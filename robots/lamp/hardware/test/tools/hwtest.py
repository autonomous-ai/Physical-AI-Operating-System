#!/usr/bin/env python3
"""
hwtest.py - hardware self-test for the OrangePi 4 Pro (sun60iw2) "lamp".

Tests: LED, touch (TTP223), button, mic, speaker, camera, servo.
Grounded in the autonomous-os HAL drivers (os/hal):
  LED      WS2812 over SPI3.0 (/dev/spidev3.0, pin 19)     <- drivers/rgb/_StripSPI
  touch    TTP223 gpiochip0 lines [96,98,100] (lgpio)      <- drivers/ttp223.py
  button   gpiochip1 line 9, active-low (lgpio)            <- drivers/gpio_button.py
  mic      arecord plug:device_micro2(16k)/micro1(44100)   <- drivers/voice, drivers/sensing
  speaker  aplay/ffmpeg plug:device_speaker (44100)        <- drivers/voice/music_service
  camera   USB UVC /dev/video0 MJPEG 640x480 (OpenCV)      <- devices/video_capture_device
  servo    Feetech STS3215 x5 on /dev/ttyACM0 (lerobot)    <- follower/hal_follower

Run all:          sudo python3 hwtest.py
Run some:         sudo python3 hwtest.py --only led,mic,speaker
Non-interactive:  sudo python3 hwtest.py --yes         (auto-checks only; no human confirms)
Root is needed for /dev/spidev* and /dev/gpiochip*.

Any component whose hardware or python deps are missing is reported SKIP, not FAIL,
so this is safe to run on a partially-populated board.
"""
import argparse, glob, os, shutil, subprocess, sys, time

RESULTS = []


def record(name, status, detail=""):
    RESULTS.append((name, status, detail))
    icon = {"PASS": "[PASS]", "FAIL": "[FAIL]", "SKIP": "[skip]"}.get(status, "[    ]")
    print(f"  {icon} {name}: {detail}")


def ask(prompt, assume_yes):
    if assume_yes:
        return True
    try:
        return input(f"{prompt} [Y/n] ").strip().lower() in ("", "y", "yes")
    except EOFError:
        return True


# ---------------------------------------------------------------- LED (WS2812)
def test_led(args):
    node = f"/dev/spidev{args.led_bus}.{args.led_dev}"
    if not os.path.exists(node):
        return record("LED", "SKIP", f"{node} missing (enable SPI3 overlay)")
    try:
        import spidev
    except ImportError:
        return record("LED", "SKIP", "python3-spidev not installed")

    BIT0, BIT1, PRIMER, RESET = 0xC0, 0xF8, 10, 250     # from _StripSPI
    br = args.brightness
    spi = spidev.SpiDev()
    spi.open(args.led_bus, args.led_dev)
    spi.max_speed_hz = 6_400_000
    spi.mode = 0

    def enc(b):
        return [BIT1 if b & (1 << (7 - i)) else BIT0 for i in range(8)]

    def show(pixels):
        buf = [0x00] * PRIMER
        for r, g, b in pixels:                          # WS2812 = GRB order
            buf += enc(int(g * br)) + enc(int(r * br)) + enc(int(b * br))
        buf += [0x00] * RESET
        spi.writebytes2(buf)

    try:
        for name, c in (("red", (255, 0, 0)), ("green", (0, 255, 0)),
                        ("blue", (0, 0, 255)), ("white", (255, 255, 255))):
            print(f"      LED -> {name}")
            show([c] * args.led_count)
            time.sleep(0.8)
        show([(0, 0, 0)] * args.led_count)              # off
        spi.close()
    except Exception as e:
        return record("LED", "FAIL", str(e))

    if ask("      Did the LED cycle red/green/blue/white?", args.yes):
        record("LED", "PASS", f"{args.led_count} px on {node}")
    else:
        record("LED", "FAIL", "no light - check 5V/GND/DIN on pin 19")


# ------------------------------------------------------- touch (TTP223) + button
def test_touch_button(args):
    try:
        import lgpio
    except ImportError:
        record("touch", "SKIP", "python3-lgpio not installed")
        record("button", "SKIP", "python3-lgpio not installed")
        return

    TOUCH_LINES = [96, 98, 100]
    NAMES = {96: "T1", 98: "T2", 100: "T3"}
    seen, btn = set(), False
    th = bh = None
    try:
        th = lgpio.gpiochip_open(0)
        for l in TOUCH_LINES:
            lgpio.gpio_claim_input(th, l, lgpio.SET_PULL_UP)
        bh = lgpio.gpiochip_open(1)
        lgpio.gpio_claim_input(bh, 9, lgpio.SET_PULL_UP)
    except Exception as e:
        if th: lgpio.gpiochip_close(th)
        if bh: lgpio.gpiochip_close(bh)
        record("touch", "FAIL", f"claim lines: {e}")
        record("button", "FAIL", f"claim line: {e}")
        return

    window = 2 if args.yes else 20
    print(f"      Touch & HOLD each pad (T1 T2 T3) and press the button - {window}s window")
    t_end = time.time() + window
    try:
        while time.time() < t_end:
            for l in TOUCH_LINES:                        # pads rest HIGH(1), touch->LOW(0)
                if lgpio.gpio_read(th, l) == 0:
                    seen.add(NAMES[l])
            if lgpio.gpio_read(bh, 9) == 0:              # button active-low
                btn = True
            print(f"        touched: {'+'.join(sorted(seen)) or '-':10}  "
                  f"button: {'DOWN' if btn else 'up'}   ", end="\r")
            time.sleep(0.02)
    finally:
        print()
        lgpio.gpiochip_close(th)
        lgpio.gpiochip_close(bh)

    miss = "SKIP" if args.yes else "FAIL"              # nobody to touch in --yes mode
    record("touch", "PASS" if seen else miss,
           f"detected {sorted(seen)}" if seen else "no pad went LOW (hold finger longer)")
    record("button", "PASS" if btn else miss,
           "press detected" if btn else "no press seen")


# ---------------------------------------------------------------------- mic
def _wav_rms(path):
    try:
        import wave, audioop
        with wave.open(path, "rb") as w:
            data = w.readframes(w.getnframes())
            width = w.getsampwidth()
        return audioop.rms(data, width)
    except Exception:
        return "n/a"


def test_mic(args):
    if not shutil.which("arecord"):
        return record("mic", "SKIP", "arecord (alsa-utils) not installed")
    attempts = [(args.mic, args.mic_rate)] if args.mic else \
               [("plug:device_micro2", 16000), ("plug:device_micro1", 44100)]
    out = "/tmp/mic_test.wav"
    last = ""
    for dev, rate in attempts:
        print(f"      recording 3s from {dev} @ {rate}Hz ...")
        p = subprocess.run(["arecord", "-D", dev, "-f", "S16_LE", "-r", str(rate),
                            "-c", "1", "-d", "3", out],
                           capture_output=True, text=True)
        if p.returncode == 0 and os.path.exists(out) and os.path.getsize(out) > 1000:
            return record("mic", "PASS",
                          f"{dev}@{rate}Hz -> {out} ({os.path.getsize(out)}B, RMS~{_wav_rms(out)})")
        err = (p.stderr or "").strip().splitlines()
        last = f"{dev}: {err[-1][:70] if err else 'no data'}"
    record("mic", "FAIL", last)


# ------------------------------------------------------------------- speaker
def test_speaker(args):
    if not shutil.which("aplay"):
        return record("speaker", "SKIP", "aplay (alsa-utils) not installed")
    dev = args.speaker
    if shutil.which("ffmpeg"):
        print("      playing 440Hz tone ...")
        tone = subprocess.Popen(
            ["ffmpeg", "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
             "-ar", "44100", "-f", "wav", "-", "-loglevel", "quiet"],
            stdout=subprocess.PIPE)
        p = subprocess.run(["aplay", "-D", dev, "-q"], stdin=tone.stdout,
                           capture_output=True, text=True)
        tone.wait()
        what = "440Hz tone"
    elif os.path.exists("/tmp/mic_test.wav"):
        print("      playing back /tmp/mic_test.wav ...")
        p = subprocess.run(["aplay", "-D", dev, "-q", "/tmp/mic_test.wav"],
                           capture_output=True, text=True)
        what = "mic recording"
    else:
        return record("speaker", "SKIP", "no ffmpeg and no /tmp/mic_test.wav to play")

    if p.returncode != 0:
        err = (p.stderr or "").strip().splitlines()
        return record("speaker", "FAIL", f"{dev}: {err[-1][:70] if err else 'aplay failed'}")
    if ask(f"      Did you hear the {what}?", args.yes):
        record("speaker", "PASS", dev)
    else:
        record("speaker", "FAIL", "no sound - check amp/speaker wiring & ES8389 mixer")


# -------------------------------------------------------------------- camera
def test_camera(args):
    if not glob.glob("/dev/video*"):
        return record("camera", "SKIP", "no /dev/video* (USB cam absent / CSI overlay off)")
    try:
        import cv2
    except ImportError:
        return record("camera", "SKIP", "opencv-python not installed")
    cap = cv2.VideoCapture(args.cam_index, cv2.CAP_V4L2)
    if not cap.isOpened():
        cap = cv2.VideoCapture(args.cam_index)
    if not cap.isOpened():
        return record("camera", "FAIL", f"cannot open index {args.cam_index}")
    cap.set(cv2.CAP_PROP_FOURCC, cv2.VideoWriter_fourcc(*"MJPG"))
    cap.set(cv2.CAP_PROP_FRAME_WIDTH, 640)
    cap.set(cv2.CAP_PROP_FRAME_HEIGHT, 480)
    ok = frame = None
    for _ in range(5):                                   # warm-up: first UVC frame is often stale
        ok, frame = cap.read()
        time.sleep(0.1)
    cap.release()
    if not ok or frame is None:
        return record("camera", "FAIL", "frame read failed")
    cv2.imwrite("/tmp/cam_test.jpg", frame)
    record("camera", "PASS", f"index {args.cam_index} -> /tmp/cam_test.jpg {frame.shape}")


# --------------------------------------------------------------------- servo
def test_servo(args):
    port = args.servo_port
    if not os.path.exists(port):
        return record("servo", "SKIP", f"{port} absent (USB Feetech controller not plugged in)")
    try:
        sys.path.insert(0, args.hal_path)                # so `import hal.*` resolves
        from hal.follower import LeLampFollower, LeLampFollowerConfig
    except Exception as e:
        return record("servo", "SKIP", f"lerobot/hal not importable: {str(e)[:60]}")
    try:
        cfg = LeLampFollowerConfig(port=port, id="hal", use_degrees=True)
        robot = LeLampFollower(cfg)
        robot.connect(calibrate=False)                   # loads repo hal.json, torque on
        j = "base_yaw.pos"
        center = robot.get_observation()[j]
        for tgt in (center + 8, center - 8, center):     # small safe nudge
            robot.send_action({j: tgt})
            time.sleep(1.0)
        robot.disconnect()                               # torque off
        record("servo", "PASS", f"moved base_yaw +/-8deg on {port}")
    except Exception as e:
        try:
            robot.disconnect()
        except Exception:
            pass
        record("servo", "FAIL", str(e)[:100])


ALL = ["led", "touch", "button", "mic", "speaker", "camera", "servo"]


def main():
    ap = argparse.ArgumentParser(description="OrangePi lamp hardware self-test")
    ap.add_argument("--only", default="", help="comma list: " + ",".join(ALL))
    ap.add_argument("-y", "--yes", action="store_true",
                    help="non-interactive: skip human confirmations")
    ap.add_argument("--led-bus", type=int, default=3)
    ap.add_argument("--led-dev", type=int, default=0)
    ap.add_argument("--led-count", type=int, default=64)
    ap.add_argument("--brightness", type=float, default=0.3)
    ap.add_argument("--mic", default="", help="override mic ALSA device")
    ap.add_argument("--mic-rate", type=int, default=44100)
    ap.add_argument("--speaker", default="plug:device_speaker")
    ap.add_argument("--cam-index", type=int, default=0)
    ap.add_argument("--servo-port", default="/dev/ttyACM0")
    ap.add_argument("--hal-path", default="/home/orangepi/autonomous/os/hal")
    args = ap.parse_args()

    sel = [c.strip() for c in args.only.split(",") if c.strip()] or ALL
    bad = [c for c in sel if c not in ALL]
    if bad:
        sys.exit(f"unknown component(s): {bad}; valid: {ALL}")

    if os.geteuid() != 0 and ({"led", "touch", "button"} & set(sel)):
        print("WARN: not root - /dev/spidev* and /dev/gpiochip* need sudo; those may fail.\n")

    print(f"== OrangePi lamp hardware self-test ==  running: {sel}\n")
    gpio_done = False
    for c in sel:
        try:
            if c in ("touch", "button"):
                if not gpio_done:
                    test_touch_button(args)
                    gpio_done = True
            elif c == "led":
                test_led(args)
            elif c == "mic":
                test_mic(args)
            elif c == "speaker":
                test_speaker(args)
            elif c == "camera":
                test_camera(args)
            elif c == "servo":
                test_servo(args)
        except KeyboardInterrupt:
            record(c, "SKIP", "interrupted")
        except Exception as e:
            record(c, "FAIL", f"unexpected: {e}")

    print("\n== summary ==")
    width = max(len(n) for n, _, _ in RESULTS)
    for n, s, d in RESULTS:
        print(f"  {n:<{width}}  {s:<4}  {d}")
    n_fail = sum(1 for _, s, _ in RESULTS if s == "FAIL")
    n_pass = sum(1 for _, s, _ in RESULTS if s == "PASS")
    n_skip = sum(1 for _, s, _ in RESULTS if s == "SKIP")
    print(f"\n  {n_pass} pass, {n_fail} fail, {n_skip} skip")
    sys.exit(1 if n_fail else 0)


if __name__ == "__main__":
    main()
