#!/usr/bin/env python3
"""
lamp_fulltest.py - full hardware test for the OrangePi 4 Pro lamp.

Works on both lamp layouts (picked automatically):
  lelamp  ~/lelamp_runtime venv, calibration ~/.cache/.../lelamp_follower/lumi.json  (lamp-0c89 bench)
  hal     /opt/hal venv,         calibration /opt/hal/calibration/robots/hal_follower/hal.json
Audio always goes through the /etc/asound.conf aliases device_micro1/device_micro2/device_speaker,
so the same test covers the C-Media bench adapter and the onboard ES8389.

Runs, in order:
  camera    one full-res frame via ~/camera_capture_test.sh (SKIP if no /dev/video*),
            shown in the terminal with chafa, then a Y/n check
  led       (off by default - the progress ring already shows the LEDs work; --only led)
            WS2812 on SPI3 (/dev/spidev3.0): red, green, blue, white
  mic       record mic1 + mic2 at the same time while you SPEAK
  speaker   play back the mic1 then mic2 recordings on device_speaker
  mpr121    12-electrode cap touch (I2C, auto-found): slide over every pad
  button    GPIO buttons, header pins 11, 35, 37 (active-low, pull-up)
  ttp223    TTP223 touch pads, header pins 27, 29, 31, 33 (active-low); PASS at >= 2 of 4
  servo     LAST. Checks all 5 STS3215 answer (no calibration here - hal.calibrate is asked at
            the very end). Old behaviour, kept below for reference only:
            Calibrate all 5 STS3215: the CURRENT pose becomes the middle (raw 2047),
            torque off, the operator rotates every joint through its range (live min/max
            shown), then presses ANY 2 OF THE 3 BUTTONS to confirm. Recorded ranges are
            saved (joints not moved get middle +/- --servo-half-range) to the calibration
            file (overwritten, no backup) and written to the motors.

While it runs:
  voice     each step is announced on device_speaker from ~/lamp_voice/<step>.wav
            (pre-recorded; missing files are just silent; --no-voice to mute)
  LED ring  every ring split in half: LEFT half = progress (tests done / total, blue),
            RIGHT half = one segment per test: green pass, red fail, amber skip,
            blue = running now. The final state stays on after the test.

Usage (via the wrapper, which picks the right python and sudo):
  ~/lamp-test                       # everything, operator present
  ~/lamp-test --only mic,speaker
  ~/lamp-test --skip servo
  ~/lamp-test --yes                 # unattended: no confirms, touch tests SKIP

Exit 0 = no FAIL. Log: ~/hwtest/fulltest/<host>_<stamp>.log
"""
import argparse, array, json, math, os, re, select, shutil, socket, subprocess, sys, threading, time, wave

HOME = "/home/orangepi"
sys.path.insert(0, HOME)                        # servo_test.py, mpr121_opi_test.py live here

OUT_DIR = f"{HOME}/hwtest/fulltest"

# servo/lerobot backends: (python, cwd, follower module, calibration dir, calibration id)
BACKENDS = {
    "lelamp": (f"{HOME}/lelamp_runtime/.venv/bin/python", f"{HOME}/lelamp_runtime", "lelamp.follower",
               f"{HOME}/.cache/huggingface/lerobot/calibration/robots/lelamp_follower", "lumi"),
    "hal": ("/opt/hal/.venv/bin/python", "/opt", "hal.follower",
            "/opt/hal/calibration/robots/hal_follower", "hal"),
}


def pick_backend():
    for name, b in BACKENDS.items():
        if os.path.exists(b[0]):
            return name, b
    return None, None

# physical header pin -> kernel GPIO number (OrangePi 4 Pro, `gpio readall`)
PIN_TO_GPIO = {
    3: 35, 5: 34, 7: 356, 8: 358, 10: 359, 11: 361, 12: 360, 13: 364,
    15: 329, 16: 357, 18: 354, 19: 130, 21: 131, 22: 119, 23: 129,
    24: 140, 26: 132, 27: 37, 28: 36, 29: 96, 31: 97, 32: 101, 33: 98,
    35: 99, 36: 102, 37: 100, 38: 103, 40: 365,
}
GPIOCHIP0_LINES = 352                           # 2000000.pinctrl; the rest is gpiochip1

RESULTS = []
LOG = None
REPORT_EXTRA = []                               # extra markdown sections for the report


def say(msg=""):
    print(msg, flush=True)
    if LOG:
        LOG.write(msg + "\n"); LOG.flush()


def record(name, status, detail=""):
    RESULTS.append((name, status, detail))
    say(f"  [{status}] {name}: {detail}")


def ask(prompt, args, default=True, seconds=None):
    """Countdown prompt, single keypress (no Enter). default=True: [Y/n], auto-Y, only n = No.
    default=False: [y/N], auto-N, only y = Yes."""
    if args.yes:
        return default
    tty_in = sys.stdin.isatty()
    saved = None
    if tty_in:
        import termios, tty
        saved = termios.tcgetattr(sys.stdin)
        tty.setcbreak(sys.stdin.fileno())             # read one key without waiting for Enter
    ans = None
    try:
        for left in range(seconds or args.confirm_seconds, 0, -1):
            hint = "[Y/n]  auto-Y" if default else "[y/N]  auto-N"
            print(f"\r  {prompt} {hint} in {left}s ", end="", flush=True)
            if select.select([sys.stdin], [], [], 1.0)[0]:
                ch = (sys.stdin.read(1) if tty_in else sys.stdin.readline()).strip().lower()
                if ch.startswith("s"):                 # a late "skip" must never answer a question
                    continue
                ans = (not ch.startswith("n")) if default else ch.startswith("y")
                break
    except (OSError, ValueError):                     # no usable stdin -> default
        pass
    finally:
        if saved is not None:
            termios.tcsetattr(sys.stdin, termios.TCSANOW, saved)
    if ans is None:
        ans = default
    print(f"-> {'Y' if ans else 'N'}")
    if LOG:
        LOG.write(f"  {prompt} -> {'Y' if ans else 'N'}\n"); LOG.flush()
    return ans


def write_report(name, host, stamp, sel, cnt):
    """Markdown report <name>-<YYYYMMDD-HHMMSS>.md next to the log; returns its path."""
    safe = re.sub(r"[^A-Za-z0-9._-]", "_", name)
    path = f"{OUT_DIR}/{safe}-{stamp}.md"
    when = time.strftime("%Y-%m-%d %H:%M:%S", time.strptime(stamp, "%Y%m%d-%H%M%S"))
    overall = "FAIL" if cnt["FAIL"] else "PASS"
    icon = {"PASS": "✅ PASS", "FAIL": "❌ FAIL", "SKIP": "⏭️ SKIP"}
    cell = lambda t: str(t).replace("|", "\\|").replace("\n", " ")
    lines = [
        f"# Lamp test report - {name}",
        "",
        "| | |", "|---|---|",
        f"| Lamp | {name} |",
        f"| Host | {host} |",
        f"| Date | {when} |",
        f"| Tests run | {', '.join(sel)} |",
        f"| Result | **{overall}** - {cnt['PASS']} pass, {cnt['FAIL']} fail, {cnt['SKIP']} skip |",
        "",
        "## Results",
        "",
        "| # | Test | Result | Detail |",
        "|---:|---|---|---|",
    ]
    for i, (n, st, d) in enumerate(RESULTS, 1):
        lines.append(f"| {i} | {n} | {icon.get(st, st)} | {cell(d)} |")
    for title, body in REPORT_EXTRA:
        lines += ["", f"## {title}", "", body]
    lines += ["", f"Log: `{LOG.name if LOG else '-'}`", ""]
    with open(path, "w") as f:
        f.write("\n".join(lines))
    return path


def systemctl(*cmd):
    """systemctl as root: directly when root, else sudo (cached / passwordless first, then prompt)."""
    if os.geteuid() == 0:
        return subprocess.run(["systemctl", *cmd]).returncode
    if subprocess.run(["sudo", "-n", "systemctl", *cmd], stderr=subprocess.DEVNULL).returncode == 0:
        return 0
    return subprocess.run(["sudo", "systemctl", *cmd]).returncode


def stop_hal():
    """Stop hal.service (it owns the servo port, GPIO lines and LED SPI). True if it was running."""
    if subprocess.run(["systemctl", "is-active", "--quiet", "hal.service"]).returncode != 0:
        say("hal.service: not running")
        return False
    say("hal.service: running - stopping it for the test (it holds servo/GPIO/LED)")
    if systemctl("stop", "hal.service") != 0:
        say("WARN: could not stop hal.service - servo/GPIO/LED tests may fail")
        return False
    en = subprocess.run(["systemctl", "is-enabled", "hal.service"], capture_output=True, text=True).stdout.strip()
    if en == "enabled":
        say("hal.service: enabled - it comes back on the next boot")
    else:
        say(f"WARN: hal.service is '{en}', NOT enabled - it will not come back on reboot (use --restart-hal)")
    return True


VOICE_DIR = f"{HOME}/lamp_voice"
VOICE_ON = True
SPEAKER = "device_speaker"


VOICE_PROC = None


def voice_wait():
    """Block until the current voice prompt has finished (before using the mic or the speaker)."""
    global VOICE_PROC
    if VOICE_PROC is not None:
        try:
            VOICE_PROC.wait(timeout=15)
        except subprocess.TimeoutExpired:
            VOICE_PROC.kill()
        VOICE_PROC = None


def speak(key):
    """Play the pre-recorded prompt ~/lamp_voice/<key>.wav in the BACKGROUND, so the step starts
    while it is being announced. A prompt still playing is cut off (the operator may already be
    on the next step). Silent if the file is missing or voice is muted."""
    global VOICE_PROC
    path = f"{VOICE_DIR}/{key}.wav"
    if VOICE_ON and os.path.exists(path):
        if VOICE_PROC is not None and VOICE_PROC.poll() is None:
            VOICE_PROC.terminate()
            try:
                VOICE_PROC.wait(timeout=1)
            except subprocess.TimeoutExpired:
                VOICE_PROC.kill()
        VOICE_PROC = subprocess.Popen(["aplay", "-q", "-D", SPEAKER, path],
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class Ring:
    """WS2812 rings on SPI3 (32+24+12 = 68 px, HAL _StripSPI encoding). Every ring is split in
    half: left half = progress bar, right half = one segment per test (status colours)."""
    RINGS = (32, 24, 12)
    COLORS = {"PASS": (0, 255, 0), "FAIL": (255, 0, 0), "SKIP": (255, 120, 0),
              "RUN": (0, 80, 255), "PROG": (0, 120, 255), None: (0, 0, 0)}

    def __init__(self, count, brightness):
        self.spi, self.count, self.br = None, count, brightness
        try:
            import spidev
            self.spi = spidev.SpiDev(); self.spi.open(3, 0)
            self.spi.max_speed_hz = 6_400_000; self.spi.mode = 0
        except Exception:
            self.spi = None                               # no LED hardware: everything is a no-op

    def show(self, pixels):
        if not self.spi:
            return
        enc = lambda b: [0xF8 if b & (1 << (7 - i)) else 0xC0 for i in range(8)]
        buf = [0] * 10                                    # primer
        for r, g, b in pixels[:self.count]:
            buf += enc(int(g * self.br)) + enc(int(r * self.br)) + enc(int(b * self.br))   # GRB
        self.spi.writebytes2(buf + [0] * 250)              # reset latch

    def fill(self, c):
        self.show([c] * self.count)

    def status(self, done, states):
        """done = tests finished; states = per-test 'PASS'/'FAIL'/'SKIP'/'RUN'/None."""
        if not states:
            return self.fill((0, 0, 0))
        total = len(states)
        px = []
        for n in self.RINGS:
            half = n // 2
            lit = round(half * done / total)
            px += [self.COLORS["PROG"] if i < lit else (0, 0, 0) for i in range(half)]
            right = []
            for j in range(n - half):                      # LED j -> which test segment
                right.append(self.COLORS[states[min(len(states) - 1, j * total // (n - half))]])
            px += right
        self.show(px + [(0, 0, 0)] * (self.count - len(px)))

    def close(self):
        if self.spi:
            self.spi.close()


RING = None


class SkipStep(Exception):
    """Operator skipped the running step (triple click on the hub button, or 's' + Enter)."""


def skip_requested():
    """Non-blocking: True if a line containing 's' arrived on the terminal (canonical mode)."""
    try:
        if sys.stdin.isatty() and select.select([sys.stdin], [], [], 0)[0]:
            return "s" in sys.stdin.readline().lower()
    except (OSError, ValueError):
        pass
    return False


def check_skip():
    if skip_requested():
        raise SkipStep()


def flush_input():
    """Drop typed-ahead input so a late skip / stray key cannot hit the next step."""
    try:
        import termios
        if sys.stdin.isatty():
            termios.tcflush(sys.stdin, termios.TCIFLUSH)
    except Exception:
        pass


def header(title):
    say(f"\n=== {title} " + "=" * max(0, 60 - len(title)))


# ===================================================================== servo
SERVO_CALIB = r'''
import importlib, json, sys, time
from pathlib import Path
from lerobot.motors import MotorCalibration
from lerobot.motors.feetech import OperatingMode

import select
port, lamp_id, half, interactive, module, cal_dir = (sys.argv[1], sys.argv[2], int(sys.argv[3]),
                                                     sys.argv[4] == "1", sys.argv[5], sys.argv[6])
fol = importlib.import_module(module)
f = fol.LeLampFollower(fol.LeLampFollowerConfig(port=port, id=lamp_id, calibration_dir=Path(cal_dir)))
f.bus.connect()
try:
    f.bus.disable_torque()
    for m in f.bus.motors:
        f.bus.write("Operating_Mode", m, OperatingMode.POSITION.value)
    homing = f.bus.set_half_turn_homings()          # current pose -> 2047, no prompt
    mid = {m: 2047 for m in f.bus.motors}
    mins = {m: max(0, 2047 - half) for m in f.bus.motors}
    maxs = {m: min(4095, 2047 + half) for m in f.bus.motors}
    if interactive:                                  # record ranges until the parent says "stop"
        print("READY", flush=True)
        lo = dict(mid); hi = dict(mid); last = 0
        while True:
            pos = f.bus.sync_read("Present_Position", normalize=False)
            for m, p in pos.items():
                lo[m] = min(lo[m], p); hi[m] = max(hi[m], p)
            if time.time() - last > 0.3:                 # live view for the operator (stderr = tty)
                last = time.time()
                sys.stderr.write("\r    " + "  ".join(f"{m.split('_')[0][:5]}:{lo[m]}-{hi[m]}" for m in f.bus.motors) + "  ")
                sys.stderr.flush()
            if select.select([sys.stdin], [], [], 0.05)[0]:
                sys.stdin.readline(); break
        sys.stderr.write("\n"); sys.stderr.flush()
        for m in f.bus.motors:                       # joint not moved -> keep default range
            if hi[m] - lo[m] >= 100:
                mins[m], maxs[m] = lo[m], hi[m]
    f.calibration = {m: MotorCalibration(id=mo.id, drive_mode=0, homing_offset=homing[m],
                                         range_min=mins[m], range_max=maxs[m])
                     for m, mo in f.bus.motors.items()}
    f.bus.write_calibration(f.calibration)
    f._save_calibration()
    pos = f.bus.sync_read("Present_Position", normalize=False)
    print("RESULT " + json.dumps({"file": str(f.calibration_fpath), "pos": pos,
          "cal": {m: [c.homing_offset, c.range_min, c.range_max] for m, c in f.calibration.items()}}),
          flush=True)
finally:
    f.bus.disconnect(disable_torque=True)
'''


def test_servo(args):
    if not os.path.exists(args.servo_port):
        return record("servo", "SKIP", f"{args.servo_port} missing (Feetech USB board not plugged)")
    try:
        from servo_test import Bus, JOINTS
    except Exception as e:
        return record("servo", "SKIP", f"cannot import ~/servo_test.py: {e}")

    # 0. are they all on the bus?
    bus = Bus(args.servo_port, args.servo_baud)
    missing = [s for s in JOINTS if bus.read_pos(s) is None]
    bus.close()
    if missing:
        return record("servo", "FAIL", f"no response from id {missing} (servo power / bus wiring)")

    # Presence check only. Calibration is done by hal.calibrate (asked at the end of the run), so
    # /opt/hal/calibration/robots/hal_follower/hal.json stays the repo default.
    return record("servo", "PASS", "all 5 servos respond (calibration: hal.calibrate at the end)")

    # 1. calibration (lerobot lives in the lelamp or hal venv)
    name, backend = pick_backend()
    if not backend:
        return record("servo-calibrate", "SKIP", "no lelamp_runtime or /opt/hal venv with lerobot")
    py, cwd, module, cal_dir, cal_id = backend
    cal_id = args.lamp_id or cal_id
    interactive = not args.yes
    say(f"  calibrating ({name}): CURRENT pose = middle of range - torque goes OFF, support the arm")
    if interactive:
        say("  ROTATE every joint through its full range, then press ANY 2 OF THE 3 BUTTONS to confirm")
    env = dict(os.environ, PYTHONPATH=cwd)
    p = subprocess.Popen([py, "-c", SERVO_CALIB, args.servo_port, cal_id, str(args.servo_half_range),
                          "1" if interactive else "0", module, cal_dir],
                         cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    confirmed = True
    if interactive:
        line = p.stdout.readline()                       # wait until torque is off and homing is set
        if line.strip() != "READY":
            p.kill()
            return record("servo-calibrate", "FAIL", f"calibration did not start ({line.strip() or 'exit'})"
                                                       " - see output above")
        seen = wait_pins(args.button_pins, 2, args.servo_timeout, "confirm")
        confirmed = seen is not None and len(seen) >= 2
        if not confirmed:
            say(f"  no 2-button confirm within {args.servo_timeout}s - saving what was recorded anyway")
        try:
            p.stdin.write("stop\n"); p.stdin.flush()
        except BrokenPipeError:
            pass
    try:
        out, _ = p.communicate(timeout=60)
    except subprocess.TimeoutExpired:
        p.kill(); out = ""
    res = None
    for line in (out or "").splitlines():
        if line.startswith("RESULT "):
            res = json.loads(line[7:])
    if p.returncode != 0 or not res:
        return record("servo-calibrate", "FAIL", f"calibration exit {p.returncode} - see output above")
    rows = ["| Joint | Homing offset | Range min | Range max | Position now |",
            "|---|---:|---:|---:|---:|"]
    for m, (off, lo, hi) in res["cal"].items():
        say(f"    {m:<12} homing_offset {off:>5}  range {lo}-{hi}  now {res['pos'][m]}")
        rows.append(f"| {m} | {off} | {lo} | {hi} | {res['pos'][m]} |")
    REPORT_EXTRA.append(("Servo calibration", "\n".join(rows)))
    if confirmed:
        speak("servo_done")
        record("servo-calibrate", "PASS", f"saved {res['file']}")
    else:
        speak("servo_timeout")
        record("servo-calibrate", "FAIL", f"operator did not confirm with 2 buttons; ranges saved anyway to {res['file']}")
    if os.geteuid() == 0 and cal_dir.startswith(HOME):   # keep ~/.cache owned by orangepi
        subprocess.run(["chown", "-R", "orangepi:orangepi", cal_dir])


# ==================================================================== camera
def test_camera(args):
    import glob
    if not glob.glob("/dev/video*"):
        return record("camera", "SKIP", "no /dev/video* - camera not plugged in")
    script = f"{HOME}/camera_capture_test.sh"
    if not os.path.exists(script):
        return record("camera", "SKIP", f"{script} missing")
    p = subprocess.run(["bash", script], capture_output=True, text=True, timeout=90)
    last = (p.stdout or p.stderr).strip().splitlines()
    detail = last[-1] if last else f"exit {p.returncode}"
    if p.returncode != 0:
        return record("camera", "FAIL", detail)
    img = os.path.realpath(f"{HOME}/hwtest/camera/latest.jpg")
    if args.yes or not os.path.exists(img):
        return record("camera", "PASS", detail)
    # text-mode preview so the operator (terminal, LCD dashboard) can see it is a real picture
    if not show_image(img, args.preview_width, not args.preview_256):
        say("  (no preview: neither OpenCV nor chafa on this lamp)")
    say(f"  full-res image: {img}  (the hub copies it next to the report)")
    ok = ask("Does the camera image look right?", args)
    record("camera", "PASS" if ok else "FAIL", detail if ok else f"operator rejected image {os.path.basename(img)}")


def show_image(path, width=104, truecolor=True):
    """Print a JPEG as half-block art (2 pixels per character cell), 24-bit colour by default.
    OpenCV first (in the HAL venv on every lamp), chafa as fallback. Sized to the smaller of
    `width` and this terminal, so it never wraps (the LCD dashboard is ~128x42 at font 12).
    Returns False if neither OpenCV nor chafa is available."""
    term = shutil.get_terminal_size((128, 42))
    try:
        import cv2
        im = cv2.imread(path)
        if im is None:
            return False
        h0, w0 = im.shape[:2]
        width = min(width, term.columns - 2)
        rows = round(width * h0 / w0 / 2)                    # half-block pixels are ~square
        if rows > term.lines - 3:                            # keep the prompt line visible
            rows = max(4, term.lines - 3); width = round(rows * 2 * w0 / h0)
        im = cv2.resize(im, (width, rows * 2), interpolation=cv2.INTER_AREA)
        im = cv2.addWeighted(im, 1.4, cv2.GaussianBlur(im, (0, 0), 1.0), -0.4, 0)   # mild sharpen
        if truecolor:
            fg = lambda p: f"38;2;{p[2]};{p[1]};{p[0]}"
            bg = lambda p: f"48;2;{p[2]};{p[1]};{p[0]}"
        else:
            q = lambda p: 16 + 36 * round(int(p[2]) / 51) + 6 * round(int(p[1]) / 51) + round(int(p[0]) / 51)
            fg = lambda p: f"38;5;{q(p)}"
            bg = lambda p: f"48;5;{q(p)}"
        out = []
        for y in range(0, rows * 2, 2):
            out.append(" " + "".join(f"\x1b[{fg(im[y, x])};{bg(im[y + 1, x])}m\u2580"
                                     for x in range(width)) + "\x1b[0m")
        print("\n".join(out), flush=True)
        return True
    except ImportError:
        pass
    if shutil.which("chafa"):
        subprocess.run(["chafa", "--colors", "full" if truecolor else "256",
                        "--size", f"{min(width, term.columns - 2)}x{term.lines - 3}", path])
        return True
    return False


# ======================================================================= led
def test_led(args):
    node = "/dev/spidev3.0"
    if not os.path.exists(node):
        return record("led", "SKIP", f"{node} missing")
    if not RING or not RING.spi:
        return record("led", "SKIP", "python3-spidev not installed / SPI open failed")
    for name, c in (("red", (255, 0, 0)), ("green", (0, 255, 0)),
                    ("blue", (0, 0, 255)), ("white", (255, 255, 255))):
        say(f"  LED -> {name}")
        RING.fill(c); time.sleep(1.0)
    RING.fill((0, 0, 0))
    ok = ask("Did the LEDs show red, green, blue, white?", args)
    record("led", "PASS" if ok else "FAIL", f"{args.led_count} px on {node}" if ok else "operator saw no/wrong colours")


# ======================================================================= mic
MIC_WAVS = {}


def wav_levels(path):
    """(peak_db, rms_db) of a 16-bit mono wav, skipping the first 0.3 s (USB start click)."""
    with wave.open(path) as w:
        w.readframes(int(w.getframerate() * 0.3))
        a = array.array("h", w.readframes(w.getnframes()))
    if not a:
        return -99.0, -99.0
    peak = max(abs(min(a)), max(a)) or 1
    rms = math.sqrt(sum(x * x for x in a) / len(a)) or 1
    return 20 * math.log10(peak / 32768), 20 * math.log10(rms / 32768)


def alias_card(alias):
    """ALSA card behind a pcm.<alias> block in /etc/asound.conf (None if not defined)."""
    try:
        text = open("/etc/asound.conf").read()
    except OSError:
        return None
    text = re.sub(r"#[^\n]*", "", text)                  # comments can mention other cards
    m = re.search(rf"pcm\.{re.escape(alias)}\s*\{{", text)
    if not m:
        return None
    m = re.search(r"\bcard\s+([\w-]+)", text[m.end():])   # works for one-line and multi-line blocks
    return m.group(1) if m else None


def test_mic(args):
    if not shutil.which("arecord"):
        return record("mic", "SKIP", "arecord missing")
    stamp = time.strftime("%Y%m%d-%H%M%S")
    procs = {}
    voice_wait()                                         # don't record the "please speak now" prompt
    say(f"  >>> SPEAK near the lamp (count 1-2-3...) - recording {args.mic_seconds}s <<<")
    time.sleep(1.0)
    for label, alias in (("mic1", "device_micro1"), ("mic2", "device_micro2")):
        card = alias_card(alias)
        if not card:
            record(label, "FAIL", f"pcm.{alias} not defined in /etc/asound.conf")
            continue
        if not os.path.exists(f"/proc/asound/{card}"):
            record(label, "FAIL", f"{alias}: ALSA card {card} not present (USB mic unplugged?)")
            continue
        rate = "44100" if card == "sndi2s4" else "48000"     # onboard ES8389 only captures at 44.1k
        path = f"{OUT_DIR}/{socket.gethostname()}_{stamp}_{label}.wav"
        say(f"    {label}: {alias} -> card {card} @ {rate} Hz")
        procs[label] = (path, subprocess.Popen(
            ["arecord", "-q", "-D", alias, "-f", "S16_LE", "-r", rate, "-c", "1",
             "-d", str(args.mic_seconds), path], stderr=subprocess.PIPE, text=True))
    t_end = time.time() + args.mic_seconds + 10
    while any(p.poll() is None for _, p in procs.values()) and time.time() < t_end:
        if skip_requested():
            for _, p in procs.values():
                p.terminate()
            raise SkipStep()
        time.sleep(0.1)
    for label, (path, p) in procs.items():
        _, err = p.communicate(timeout=5)
        if p.returncode != 0 or not os.path.exists(path):
            record(label, "FAIL", f"arecord failed: {(err or '').strip()[:100]}")
            continue
        peak, rms = wav_levels(path)
        MIC_WAVS[label] = path
        if peak < args.mic_min_peak:
            record(label, "FAIL", f"too quiet: peak {peak:.1f} dB < {args.mic_min_peak} dB (rms {rms:.1f})")
        elif peak > -0.5:                                # pinned at full scale = noise/gain fault
            record(label, "FAIL", f"clipping: peak {peak:.1f} dB, rms {rms:.1f} dB "
                                  f"(gain too high / floating input) -> {os.path.basename(path)}")
        else:
            record(label, "PASS", f"peak {peak:.1f} dB, rms {rms:.1f} dB -> {os.path.basename(path)}")


# =================================================================== speaker
def test_speaker(args):
    if not MIC_WAVS:
        return record("speaker", "SKIP", "no mic recordings to play (run the mic test first)")
    voice_wait()                                         # speaker plays one sound at a time
    for label in ("mic1", "mic2"):
        if label not in MIC_WAVS:
            continue
        say(f"  playing {label} recording on {args.speaker} ...")
        p = subprocess.Popen(["aplay", "-q", "-D", args.speaker, MIC_WAVS[label]],
                             stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        while p.poll() is None:
            if skip_requested():
                p.terminate()
                raise SkipStep()
            time.sleep(0.1)
        p.stderr_text = p.stderr.read()
        if p.returncode != 0:
            return record("speaker", "FAIL", f"aplay: {p.stderr_text.strip()[:100]}")
        time.sleep(0.5)
    ok = ask(f"Did you hear the {' and '.join(sorted(MIC_WAVS))} recording(s)?", args)
    record("speaker", "PASS" if ok else "FAIL", args.speaker if ok else "operator heard nothing")


# ==================================================================== mpr121
def test_mpr121(args):
    try:
        import mpr121_opi_test as m
    except Exception as e:
        return record("mpr121", "SKIP", f"cannot import ~/mpr121_opi_test.py: {e}")
    bus_num, addr = m.find_mpr121()
    if bus_num is None:
        return record("mpr121", "FAIL", "no MPR121 on any I2C bus (0x5A-0x5D)")
    bus = m.I2CBus(bus_num)
    try:
        cap = m.MPR121(bus, addr)
        cap.begin(touch_th=args.mpr_touch, release_th=args.mpr_release, debounce=2)
        time.sleep(0.3)                                  # let autoconfig + baseline settle
        oor = cap.out_of_range()
        want = set(range(12)) - set(args.mpr_ignore)
        if args.yes:
            return record("mpr121", "SKIP", f"found on i2c-{bus_num} 0x{addr:02X}; no operator to touch")
        say(f"  MPR121 on i2c-{bus_num} 0x{addr:02X} (touch={args.mpr_touch} release={args.mpr_release})")
        say(f"  slide your finger over ALL pads ({args.seconds}s) ...")
        seen, last, t_end = set(), 0, time.time() + args.seconds
        peak = [0] * 12                                  # strongest baseline-filtered delta per pad
        while time.time() < t_end and not want <= seen:
            check_skip()
            cur = cap.touched()
            filt, base = cap.all_filtered(), cap.all_baseline()
            for i in range(12):
                peak[i] = max(peak[i], base[i] - filt[i])
                if cur & (1 << i) and not last & (1 << i):
                    seen.add(i)
                    say(f"    electrode {i:2d} touched (delta {base[i] - filt[i]})   ({len(seen & want)}/{len(want)})")
            last = cur
            time.sleep(0.01)
    finally:
        bus.close()
    miss = sorted(want - seen)
    oor_l = [i for i in range(12) if oor & (1 << i)]
    note = f"  (autoconfig out-of-range: {oor_l})" if oor_l else ""
    weak = ", ".join(f"{i}:{peak[i]}" for i in miss)   # best delta reached vs touch threshold
    record("mpr121", "PASS" if not miss else "FAIL",
           (f"all {len(want)} electrodes" if not miss else
            f"never touched: {miss}  best delta {weak} (need > {args.mpr_touch})") + note)


# ============================================================ gpio (lgpio)
def chip_line(pin):
    g = PIN_TO_GPIO[pin]
    return (0, g) if g < GPIOCHIP0_LINES else (1, g - GPIOCHIP0_LINES)


def wait_pins(pins, need, seconds, what):
    """Claim pins as active-low inputs with pull-up and wait until `need` different pins saw a
    falling edge or `seconds` pass. Returns the set of pins seen, or None if lgpio/claim failed."""
    try:
        import lgpio
    except ImportError:
        say("  python3-lgpio not installed")
        return None
    handles, cbs, seen = {}, [], set()
    lock = threading.Lock()
    ignore_until = time.monotonic() + 0.5                # lgpio reports the resting level on claim
    label = {}

    def on_edge(chip, line, level, tick):
        if level != 0 or time.monotonic() < ignore_until:
            return
        pin = label[(chip, line)]
        with lock:
            if pin in seen:
                return
            seen.add(pin)
        say(f"    pin {pin} {what}   ({len(seen)}/{len(pins)}, need {need})")
    try:
        for pin in pins:
            chip, line = chip_line(pin)
            if chip not in handles:
                handles[chip] = lgpio.gpiochip_open(chip)
            h = handles[chip]
            lgpio.gpio_claim_alert(h, line, lgpio.FALLING_EDGE, lgpio.SET_PULL_UP)
            label[(chip, line)] = pin
            cbs.append(lgpio.callback(h, line, lgpio.FALLING_EDGE,
                                      lambda c, l, lv, t, chip=chip: on_edge(chip, l, lv, t)))
    except Exception as e:
        for h in handles.values():
            lgpio.gpiochip_close(h)
        say(f"  GPIO claim failed: {e} (is hal.service holding the lines?)")
        return None
    say(f"  {what.upper()} pins {pins} - need {need} of {len(pins)} ({seconds}s) ...")
    t_end = time.time() + seconds
    try:
        while time.time() < t_end and len(seen) < need:
            check_skip()
            time.sleep(0.05)
    finally:
        for cb in cbs:
            cb.cancel()
        for h in handles.values():
            lgpio.gpiochip_close(h)
    return seen


def watch_pins(args, name, pins, what, need=None):
    """PASS once `need` pins (default: all) saw a press/touch within --seconds."""
    need = min(need or len(pins), len(pins))
    if args.yes:
        return record(name, "SKIP", "no operator")
    seen = wait_pins(pins, need, args.seconds, what)
    if seen is None:
        return record(name, "FAIL", "GPIO unavailable (lgpio missing or lines busy)")
    ok = sorted(seen)
    miss = [p for p in pins if p not in seen]
    record(name, "PASS" if len(ok) >= need else "FAIL",
           f"{len(ok)}/{len(pins)} ok {ok}" + (f", untested {miss}" if miss else "")
           + ("" if len(ok) >= need else f" - need {need}"))


def test_button(args):
    watch_pins(args, "button", args.button_pins, "press")


def test_ttp223(args):
    watch_pins(args, "ttp223", args.ttp_pins, "touch", need=args.ttp_min)


# ====================================================================== main
TESTS = {"camera": test_camera, "led": test_led, "mic": test_mic, "speaker": test_speaker,
         "mpr121": test_mpr121, "button": test_button, "ttp223": test_ttp223, "servo": test_servo}


DEFAULT_OFF = {"led"}                                # run only when named in --only


def pins(s):
    return [int(x) for x in s.replace(",", " ").split()]


def main():
    global LOG
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", default="", help="comma list of: " + ",".join(TESTS))
    ap.add_argument("--name", default="", help="lamp name for the report file (default: hostname)")
    ap.add_argument("--restart-hal", action="store_true",
                    help="start hal.service again after the test (default: leave it stopped until next boot)")
    ap.add_argument("--skip", default="", help="comma list to leave out")
    ap.add_argument("-y", "--yes", action="store_true", help="unattended: no confirms, touch/press tests SKIP")
    ap.add_argument("--confirm-seconds", type=int, default=3,
                    help="Y/n prompts auto-answer Y after this many seconds (press n to answer No)")
    ap.add_argument("--seconds", type=int, default=20, help="window for touch/press tests")
    # servo
    ap.add_argument("--servo-port",
                    default="/dev/device-servo" if os.path.exists("/dev/device-servo") else "/dev/ttyACM0")
    ap.add_argument("--servo-baud", type=int, default=1_000_000)
    ap.add_argument("--lamp-id", default="",
                    help="calibration file name; default lumi (lelamp) or hal (HAL)")
    ap.add_argument("--servo-half-range", type=int, default=1024, help="range = 2047 +/- this (ticks)")
    ap.add_argument("--servo-timeout", type=int, default=180,
                    help="seconds to wait for the 2-button confirm after rotating the joints")
    # led / audio
    ap.add_argument("--led-count", type=int, default=68, help="32+24+12 rings")
    ap.add_argument("--brightness", type=float, default=0.3)
    ap.add_argument("--mic-seconds", type=int, default=4)
    ap.add_argument("--mic-min-peak", type=float, default=-50.0, help="dBFS a voice must reach")
    ap.add_argument("--speaker", default="device_speaker")
    ap.add_argument("--no-voice", action="store_true", help="do not announce steps on the speaker")
    ap.add_argument("--preview-width", type=int, default=104, help="camera preview width in characters")
    ap.add_argument("--preview-256", action="store_true", help="256-colour preview instead of 24-bit")
    ap.add_argument("--calibrate-seconds", type=int, default=10,
                    help="time to answer the 'calibrate servos' question (default No)")
    ap.add_argument("--calib-id", default="lumi", help="--id passed to hal.calibrate")
    ap.add_argument("--watch-seconds", type=int, default=30,
                    help="after hal.calibrate: hal runs this long before the automatic power-off (n cancels)")
    ap.add_argument("--poweroff-flag", default="",
                    help="(used by lamp-remote-test) on 'y' create this file instead of powering off now")
    # touch / buttons
    ap.add_argument("--mpr-touch", type=int, default=6,
                    help="touch threshold (counts, lower = more sensitive); idle noise on opidev1 peaks at +7")
    ap.add_argument("--mpr-release", type=int, default=3)
    ap.add_argument("--mpr-ignore", type=pins, default=[], help="electrodes not wired, e.g. 10,11")
    ap.add_argument("--button-pins", type=pins, default=[11, 35, 37])
    ap.add_argument("--ttp-pins", type=pins, default=[27, 29, 31, 33])
    ap.add_argument("--ttp-min", type=int, default=2, help="TTP223 pads that must register to PASS")
    args = ap.parse_args()

    sel = [t.strip() for t in args.only.split(",") if t.strip()] or [t for t in TESTS if t not in DEFAULT_OFF]
    sel = [t for t in sel if t not in {s.strip() for s in args.skip.split(",")}]
    bad = [t for t in sel if t not in TESTS]
    if bad:
        sys.exit(f"unknown test(s) {bad}; valid: {list(TESTS)}")

    os.makedirs(OUT_DIR, exist_ok=True)
    host, stamp = socket.gethostname(), time.strftime("%Y%m%d-%H%M%S")
    LOG = open(f"{OUT_DIR}/{host}_{stamp}.log", "w")
    global RING, VOICE_ON, SPEAKER
    VOICE_ON, SPEAKER = not args.no_voice, args.speaker
    say(f"lamp full test on {host}  {stamp}   tests: {', '.join(sel)}")
    hal_was_running = stop_hal()                         # before touching SPI/GPIO/servo
    order = [t for t in TESTS if t in sel]               # fixed order, whatever --only says
    states = [None] * len(order)
    RING = Ring(args.led_count, args.brightness)
    RING.status(0, states)

    try:
        for k, t in enumerate(order):
            header(f"{t}  ({k + 1}/{len(order)})   [triple click / s+Enter = skip]")
            flush_input()
            states[k] = "RUN"
            RING.status(k, states)
            speak(t)
            first = len(RESULTS)
            try:
                TESTS[t](args)
            except SkipStep:
                record(t, "SKIP", "skipped by operator")
            except KeyboardInterrupt:
                record(t, "SKIP", "interrupted (Ctrl-C)")
            except Exception as e:
                record(t, "FAIL", f"unexpected: {type(e).__name__}: {e}")
            got = [st for _, st, _ in RESULTS[first:]]
            states[k] = "FAIL" if "FAIL" in got else "PASS" if "PASS" in got else "SKIP"
            RING.status(k + 1, states)
    finally:
        if hal_was_running and args.restart_hal:
            say("\nhal.service: starting it again (--restart-hal)")
            systemctl("start", "hal.service")
        elif hal_was_running:
            say("\nhal.service: left stopped - starts again on next boot (or: sudo systemctl start hal)")

    header("summary")
    w = max(len(n) for n, _, _ in RESULTS)
    for n, s, d in RESULTS:
        say(f"  {n:<{w}}  {s:<4}  {d}")
    cnt = {k: sum(1 for _, s, _ in RESULTS if s == k) for k in ("PASS", "FAIL", "SKIP")}
    say(f"\n  {cnt['PASS']} pass, {cnt['FAIL']} fail, {cnt['SKIP']} skip   log: {LOG.name}")
    report = write_report(args.name or host, host, stamp, order, cnt)
    say(f"  report: {report}")
    speak("done_fail" if cnt["FAIL"] else "done_pass")      # plays while the questions below count down
    RING.close()                                         # ring keeps showing the final result
    flush_input()                                        # no stray skip/keys into the questions
    poweroff = ask("Power off the lamp now?", args, default=False)

    # HAL servo calibration (interactive, lerobot-style), asked after the power-off question;
    # a requested power-off waits until it is done
    if ask("Calibrate the servos now (hal.calibrate)?", args, default=False, seconds=args.calibrate_seconds):
        ok = hal_calibrate(args, args.calib_id)
        # watch the arm idle under hal, then power off unless the operator cancels
        say("  starting hal.service - watch the servos in idle mode")
        systemctl("start", "hal.service")
        if ok:
            poweroff = ask("hal is running - check the servos. Power off the lamp now?  (n = keep it on)",
                           args, default=True, seconds=args.watch_seconds)
        else:
            say("  calibration failed - lamp stays on with hal running so you can look at it")
            poweroff = False

    if poweroff:
        if args.poweroff_flag:                           # run via the hub: it powers off after fetching the report
            open(args.poweroff_flag, "w").close()
            say("  poweroff requested - the hub turns the lamp off after collecting the report")
        else:
            say("  powering off ...")
            systemctl("poweroff")
    sys.exit(1 if cnt["FAIL"] else 0)


def hal_calibrate(args, lamp_id):
    """source /opt/hal/.venv/bin/activate && python3 -m hal.calibrate --id <id> --port <servo> --follower-only

    Runs it through a small relay so its "use the provided calibration file, or type 'c'" question
    is answered with 'c' automatically: an existing calibration is always overwritten. Everything
    else stays interactive - its output is shown live and the operator's ENTERs (keyboard, or the
    hub button: 1 click = ENTER) are passed through."""
    py = "/opt/hal/.venv/bin/python"
    if not os.path.exists(py):
        say(f"  cannot calibrate: {py} not found (no HAL on this lamp)")
        return False
    cmd = [py, "-m", "hal.calibrate", "--id", lamp_id, "--port", args.servo_port, "--follower-only"]
    say("  running: " + " ".join(cmd))
    say("  (an old calibration is overwritten automatically; ENTER = arm is in the middle / stop recording)")
    env = dict(os.environ, PYTHONPATH="/opt", PYTHONUNBUFFERED="1", VIRTUAL_ENV="/opt/hal/.venv",
               PATH="/opt/hal/.venv/bin:" + os.environ.get("PATH", ""))
    p = subprocess.Popen(cmd, cwd="/opt", env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, bufsize=0)

    def pump_out():                                      # child output -> terminal, auto-answer 'c'
        tail, answered = b"", False
        while True:
            chunk = os.read(p.stdout.fileno(), 4096)
            if not chunk:
                break
            sys.stdout.buffer.write(chunk); sys.stdout.flush()
            if LOG:
                LOG.write(chunk.decode(errors="replace")); LOG.flush()
            tail = (tail + chunk)[-400:]
            if not answered and b"type 'c'" in tail:
                answered = True
                p.stdin.write(b"c\n"); p.stdin.flush()
                say("\n  (old calibration found -> answered 'c': recalibrating, it will be overwritten)")

    out_t = threading.Thread(target=pump_out, daemon=True)
    out_t.start()
    flush_input()
    while p.poll() is None:                              # operator's lines -> child
        try:
            if sys.stdin.isatty() and select.select([sys.stdin], [], [], 0.2)[0]:
                line = sys.stdin.readline()
                if p.poll() is None:
                    p.stdin.write(line.encode()); p.stdin.flush()
            elif not sys.stdin.isatty():
                time.sleep(0.2)
        except (BrokenPipeError, OSError, ValueError):
            break
    rc = p.wait()
    out_t.join(timeout=2)
    say(f"  hal.calibrate finished (exit {rc})")
    return rc == 0

if __name__ == "__main__":
    main()
