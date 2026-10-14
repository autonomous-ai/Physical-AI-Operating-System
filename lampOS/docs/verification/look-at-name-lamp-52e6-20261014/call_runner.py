"""Mac -> lamp-52e6: say "Hey Lamp, ..." aloud and time the body's turn to the call.

Adapted from the Mac-to-lamp voice kit (lamp_v2_convo.py, 2026-10-14): `say -v
Daniel` rendered to a WAV and played with afplay from the Mac beside the lamp.
Plain `ssh orangepi@lamp-52e6.local` (a key), nothing else on the lamp is
changed by this runner. It keeps no sound: the WAVs live in a temporary
directory, and what the lamp's microphone heard is never recorded.

Per case, on the lamp's wall clock (unix ms; the Mac's is mapped to it over
SSH before each case, error = half the round trip):
  name_end      the Mac finished saying "Hey Lamp" (the phrase rendered alone
                gives its length: an estimate, `say` joins words a little
                differently in the whole sentence)
  speech_end    the Mac finished the whole sentence
  name_heard    lamp-name said name/heard (bus header)
  voice_listen  the voice's state on the bus first became listening
  head_goal     body/joint_states: the head's commanded pitch first changed
                after the Mac began (the voice's own listening may do that)
  head_moved    body/joint_states: the head's pitch first read 3 counts away
  call_goal     body/joint_states: the first commanded change of any joint
                after name_heard
  glance        body/joint_states: the base's commanded yaw first left its
                place by more than 15 counts, and its largest reach each way
  endpoint, first_write  the voice's own events (end of speech, first sound)
body/joint_states comes every 80 ms, so each body time is late by up to that.

Usage: python3 call_runner.py OUT.jsonl
"""
import base64, json, math, os, subprocess, sys, tempfile, time, wave

HOST = "orangepi@lamp-52e6.local"
BUS = "/opt/lampOS/trial/lamp-bus"
VOICE = "Daniel"
CASES = [
    ("hey-lamp-time", "Hey Lamp,", " what time is it in Tokyo?"),
    ("hey-lamp-color", "Hey Lamp,", " what color is the sky at sunset?"),
    ("hey-lamp-joke", "Hey Lamp,", " tell me a very short joke."),
]
COUNTS = 4096 / (2 * math.pi)


# One connection kept open, so that a clock reading costs a round trip and not a login.
SSH = ["ssh", "-o", "ControlMaster=auto", "-o", "ControlPath=/tmp/fifth-37-%r@%h", "-o", "ControlPersist=600", HOST]


def ssh(cmd, timeout=60):
    return subprocess.run(SSH + [cmd], capture_output=True, text=True, timeout=timeout).stdout


def offset_ms():
    """lamp_unix_ms - mac_unix_ms, and half the round trip."""
    best = None
    for _ in range(3):
        t0 = time.time()
        lamp = int(ssh("date +%s%3N"))
        t1 = time.time()
        cand = (lamp - (t0 + t1) / 2 * 1000, (t1 - t0) / 2 * 1000)
        if best is None or cand[1] < best[1]:
            best = cand
    return best


def say(text, path):
    subprocess.run(["say", "-v", VOICE, "-r", "230", "-o", path, "--data-format=LEI16@22050", text], check=True)
    with wave.open(path) as w:
        return w.getnframes() / w.getframerate()


def read_jsonl(text):
    out = {}
    for line in text.splitlines():
        try:
            o = json.loads(line)
        except ValueError:
            continue
        if "payload_b64" in o:
            h = o["header"]
            out.setdefault(h["topic"], []).append((h["at_unix_ms"], json.loads(base64.b64decode(o["payload_b64"]))))
    return out


def voice_events(since_ms, until_ms):
    """The voice's endpoint and first sound written, as unix ms, from its newest session."""
    mono_unix = ssh("python3 -c 'import time;print(time.monotonic(), time.time())'").split()
    shift = float(mono_unix[1]) * 1000 - float(mono_unix[0]) * 1000
    d = ssh("sudo sh -c 'ls -td /var/lib/lampOS/voice/sessions/*/ | head -1'").strip()
    text = ssh(f"sudo sh -c 'cat {d}metadata/events-*.jsonl'")
    found = {}
    for line in text.splitlines():
        try:
            ev = json.loads(line)
        except ValueError:
            continue
        at = ev.get("at_us", 0) / 1000 + shift
        if since_ms <= at <= until_ms and ev.get("kind") in ("local_endpoint", "speaker_first_write"):
            found.setdefault(ev["kind"], round(at))
    return found


def run_case(case, tmp):
    cid, name, rest = case
    name_s = say(name, os.path.join(tmp, "name.wav"))
    whole = os.path.join(tmp, "whole.wav")
    speech_s = say(name + rest, whole)
    off, err = offset_ms()
    rec = f"/tmp/fifth-37-call-{cid}"
    ssh(f"rm -rf {rec}; mkdir -p {rec}; (setsid {BUS} record {rec} name/heard voice/state body/joint_states --seconds 30 >/dev/null 2>&1 &)")
    time.sleep(1.5)
    start = time.time() * 1000 + off
    subprocess.run(["afplay", whole], check=True)
    end = time.time() * 1000 + off
    time.sleep(29)
    topics = read_jsonl(ssh(f"cat {rec}/*.jsonl"))
    r = {"case": cid, "said": name + rest, "offset_err_ms": round(err, 1),
         "play_start": round(start), "name_end": round(start + name_s * 1000),
         "speech_end": round(start + speech_s * 1000), "play_returned": round(end)}
    heard = [at for at, p in topics.get("name/heard", []) if at >= start]
    r["name_heard"] = heard[0] if heard else None
    r["name_heard_all"] = heard
    listen = [at for at, p in topics.get("voice/state", []) if at >= start and p.get("state") == "listening"]
    r["voice_listen"] = listen[0] if listen else None
    joints = [(at, [round(x * COUNTS) for x in p["commanded"]], [round(x * COUNTS) for x in p["position"]])
              for at, p in topics.get("body/joint_states", [])]
    before = [j for j in joints if j[0] < start]
    if before:
        base = before[-1]
        after = [j for j in joints if j[0] >= start]
        goal = next((j for j in after if j[1][4] != base[1][4]), None)
        moved = next((j for j in after if abs(j[2][4] - base[2][4]) >= 3), None)
        r["head_goal"] = goal[0] if goal else None
        r["head_moved"] = moved[0] if moved else None
        r["head_pitch_from"] = base[2][4]
        r["head_pitch_reached"] = min((j[2][4] for j in after), default=None)
        r["stand_from"] = base[2][1]
        if r["name_heard"]:
            prev = [j for j in joints if j[0] <= r["name_heard"]]
            call = next((j for j in joints if j[0] > r["name_heard"] and prev and j[1] != prev[-1][1]), None)
            r["call_goal"] = call[0] if call else None
        away = [j for j in after if abs(j[1][0] - base[1][0]) > 15]
        if away:
            r["glance_goal"] = away[0][0]
            # The clip lasts a second; what comes later is the lamp thinking.
            seg = [j for j in after if away[0][0] - 200 <= j[0] <= away[0][0] + 1400]
            r["glance_yaw_read"] = [base[2][0], max(j[2][0] for j in seg), min(j[2][0] for j in seg), seg[-1][2][0]]
    r.update(voice_events(start, start + 30_000))
    ssh(f"rm -rf {rec}")
    return r


def main():
    out = sys.argv[1]
    vol = subprocess.run(["osascript", "-e", "output volume of (get volume settings)"], capture_output=True, text=True).stdout.strip()
    subprocess.run(["osascript", "-e", "set volume output volume 75"])
    try:
        with open(out, "a") as f, tempfile.TemporaryDirectory() as tmp:
            for case in CASES:
                r = run_case(case, tmp)
                f.write(json.dumps(r) + "\n"); f.flush()
                print(json.dumps(r), flush=True)
                time.sleep(15)  # past the ten seconds a call is let go for
    finally:
        subprocess.run(["osascript", "-e", f"set volume output volume {vol}"])


if __name__ == "__main__":
    main()
