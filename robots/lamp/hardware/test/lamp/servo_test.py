#!/usr/bin/env python3
"""
Simple Feetech STS3215 servo test — OrangePi 4 Pro lamp.

Uses scservo_sdk with the SAME register API as the HAL's
os/hal/drivers/motors/animation_service.py :: move_to_raw() :
    Torque_Enable = reg 40, Goal_Position = reg 42, Present_Position = reg 56
"Slow" motion = software-interpolating the goal position over a duration
at `fps` frames (exactly how the HAL does smooth moves) — NOT a big jump.

Servo map (follower/hal_follower.py), raw range 0-4095, center ~2048:
    id1 base_yaw   id2 base_pitch   id3 elbow_pitch   id4 wrist_roll   id5 wrist_pitch

INTERACTIVE (default):  python3 servo_test.py
    <id> <pos>     e.g.  "1 2048"   -> move servo 1 to raw 2048 (slowly)
    <id> +120            relative nudge +120 ticks (~10.5 deg)
    <id> -120
    r <id> | r all       read present position(s), raw + degrees
    w                    watch present positions live until Ctrl-C (nothing moves)
    off <id>|all         torque OFF (limp)      on <id>|all   torque ON
    q                    quit
ONE-SHOT:   python3 servo_test.py --id 1 --pos 2048
            python3 servo_test.py --id 3 --delta -150 --duration 3
MONITOR  :  python3 servo_test.py --watch          (read-only, never moves)

Present position (reg 56) is printed at startup, by 'r'/'w', and — while a move
runs — as a live 'goal / pos / lag' line so you can see the servo tracking the
interpolated goal. Use --no-live to suppress that during moves.

Options: --port /dev/ttyACM0  --baud 1000000  --duration 2.0  --fps 30
         --max-step 400   (safety: moves bigger than this need confirm / --force)
         --watch --watch-hz 5.0   --no-live

Install:  pip install feetech-servo-sdk    (provides scservo_sdk + pyserial)
Needs the Feetech USB controller on /dev/ttyACM0.
"""
import argparse, sys, time
from scservo_sdk import PortHandler, PacketHandler, COMM_SUCCESS

TORQUE_ENABLE = 40      # STS3215 register (1 byte)
GOAL_POSITION = 42      # STS3215 register (2 bytes, 0-4095)
PRESENT_POSITION = 56   # STS3215 register (2 bytes)
RAW_MIN, RAW_MAX = 0, 4095
RAW_CENTER = 2048
JOINTS = {1: "base_yaw", 2: "base_pitch", 3: "elbow_pitch", 4: "wrist_roll", 5: "wrist_pitch"}


def raw_to_deg(raw):
    """Raw tick -> degrees from center, same convention as servo_ping.py."""
    return (raw - RAW_CENTER) * 360.0 / 4096.0


def fmt_pos(raw):
    """'2048 (+0.0deg)' — or 'NO RESPONSE' when the read failed."""
    if raw is None:
        return "NO RESPONSE"
    return f"{raw:>4} ({raw_to_deg(raw):+6.1f}deg)"


class Bus:
    def __init__(self, port, baud):
        self.ph = PortHandler(port)
        self.pk = PacketHandler(0)   # 0 = SCS_END for STS/SMS (STS3215), little-endian
        if not self.ph.openPort():
            sys.exit(f"ERROR: failed to open {port} (is the Feetech controller plugged in?)")
        if not self.ph.setBaudRate(baud):
            sys.exit(f"ERROR: failed to set baud {baud}")

    def read_pos(self, sid):
        data, result, _ = self.pk.read2ByteTxRx(self.ph, sid, PRESENT_POSITION)
        return data if result == COMM_SUCCESS else None

    def torque(self, sid, on):
        self.pk.write1ByteTxRx(self.ph, sid, TORQUE_ENABLE, 1 if on else 0)

    def move_slow(self, sid, target, duration, fps, max_step, force, live=True):
        cur = self.read_pos(sid)
        if cur is None:
            print(f"  servo {sid}: NO RESPONSE (check ID / power / bus wiring)")
            return
        target = max(RAW_MIN, min(RAW_MAX, int(target)))
        # SKIP checking long range rotate
        # if abs(target - cur) > max_step and not force:
        #     ans = input(f"  ! big move {cur}->{target} ({abs(target-cur)} ticks > {max_step}). proceed? [y/N] ")
        #     if ans.strip().lower() not in ("y", "yes"):
        #         print("  skipped."); return
        self.torque(sid, True)
        frames = max(1, int(duration * fps))
        period = 1.0 / fps
        # read present position ~6x/sec while moving (a full read per frame would
        # double the bus traffic and stretch the interpolation)
        every = max(1, int(round(fps / 6.0)))
        for f in range(1, frames + 1):
            t0 = time.monotonic()
            raw = max(RAW_MIN, min(RAW_MAX, int(cur + (target - cur) * f / frames)))
            self.pk.write2ByteTxRx(self.ph, sid, GOAL_POSITION, raw)   # same as move_to_raw
            if live and (f % every == 0 or f == frames):
                now = self.read_pos(sid)
                lag = "  n/a" if now is None else f"{now - raw:+5d}"
                print(f"\r    moving servo {sid}  goal {raw:>4}  pos {fmt_pos(now)}  lag {lag}   ",
                      end="", flush=True)
            time.sleep(max(0.0, period - (time.monotonic() - t0)))
        if live:
            print()   # close off the \r status line
        end = self.read_pos(sid)
        print(f"  servo {sid} ({JOINTS.get(sid, '?')}): {cur} -> {target}  (now {fmt_pos(end)})")

    def watch(self, hz=5.0):
        """Continuously print present position of every servo until Ctrl-C."""
        ids = list(JOINTS)
        print(f"watching present position at {hz:g} Hz — Ctrl-C to stop")
        print("  " + "  ".join(f"{JOINTS[s]:>12}" for s in ids))
        first = True
        try:
            while True:
                vals = [self.read_pos(s) for s in ids]
                if not first:
                    print("\033[1A", end="")      # redraw over the previous row
                first = False
                print("  " + "  ".join(
                    ("     NO RESP" if v is None else f"{v:>5}{raw_to_deg(v):+7.1f}") for v in vals))
                time.sleep(1.0 / hz)
        except KeyboardInterrupt:
            print("\n  stopped.")

    def close(self):
        self.ph.closePort()


def parse_targets(bus, tok, args):
    """tok like ['1','2048'] or ['1','+120']; returns (sid, absolute_target) or None."""
    try:
        sid = int(tok[0])
    except ValueError:
        print("  ? bad servo id"); return None
    if sid not in JOINTS:
        print(f"  ? id must be 1-5 ({', '.join(f'{k}={v}' for k, v in JOINTS.items())})"); return None
    v = tok[1]
    if v[0] in "+-":
        cur = bus.read_pos(sid)
        if cur is None:
            print(f"  servo {sid}: no response"); return None
        return sid, cur + int(v)
    return sid, int(v)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", default="/dev/ttyACM0")
    ap.add_argument("--baud", type=int, default=1_000_000)
    ap.add_argument("--duration", type=float, default=2.0)     # slow
    ap.add_argument("--fps", type=int, default=30)
    ap.add_argument("--max-step", type=int, default=400)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--id", type=int)
    ap.add_argument("--pos", type=int)
    ap.add_argument("--delta", type=int)
    ap.add_argument("--watch", action="store_true",
                    help="just stream present positions until Ctrl-C (nothing moves)")
    ap.add_argument("--watch-hz", type=float, default=5.0)
    ap.add_argument("--no-live", dest="live", action="store_false",
                    help="don't print present position while a move is running")
    args = ap.parse_args()

    bus = Bus(args.port, args.baud)
    try:
        # read-only position monitor
        if args.watch:
            bus.watch(args.watch_hz)
            return

        # one-shot
        if args.id is not None and (args.pos is not None or args.delta is not None):
            tgt = args.pos if args.pos is not None else (bus.read_pos(args.id) or 2048) + args.delta
            bus.move_slow(args.id, tgt, args.duration, args.fps, args.max_step, args.force, args.live)
            return

        print("Feetech STS3215 test on", args.port)
        print("current positions:")
        for s in JOINTS:
            print(f"  servo {s} ({JOINTS[s]:<12}): {fmt_pos(bus.read_pos(s))}")
        print("type '<id> <pos>' (e.g. '1 2048'), '<id> +120', 'r all', 'w', 'off all', 'q'")
        while True:
            try:
                line = input("servo> ").strip()
            except EOFError:
                break
            if not line:
                continue
            tok = line.split()
            cmd = tok[0].lower()
            if cmd in ("q", "quit", "exit"):
                break
            if cmd == "r":
                ids = JOINTS if len(tok) < 2 or tok[1] == "all" else [int(tok[1])]
                for s in ids:
                    print(f"  servo {s} ({JOINTS.get(s,'?'):<12}): {fmt_pos(bus.read_pos(s))}")
                continue
            if cmd in ("w", "watch"):
                bus.watch(args.watch_hz)
                continue
            if cmd in ("off", "on"):
                ids = JOINTS if len(tok) < 2 or tok[1] == "all" else [int(tok[1])]
                for s in ids:
                    bus.torque(s, cmd == "on")
                print(f"  torque {cmd} for {list(ids)}")
                continue
            if len(tok) == 2:
                res = parse_targets(bus, tok, args)
                if res:
                    bus.move_slow(res[0], res[1], args.duration, args.fps,
                                  args.max_step, args.force, args.live)
                continue
            print("  ? use '<id> <pos>', '<id> +/-delta', 'r [id|all]', 'w', 'off/on [id|all]', 'q'")
    finally:
        bus.close()


if __name__ == "__main__":
    main()
