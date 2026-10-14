"""The first builder's played check, imitated, with voices no model here learned from.

    python played.py make
    python played.py score MODEL.onnx [MODEL.onnx ...] [--every N]
    python played.py verdict ../../.cache/name/runs/played-MODEL[-MODEL...].json
    python played.py summary ../../.cache/name/runs/played-MODEL[-MODEL...].json

On lamp-4ace (issue #694, 2026-10-14) six calls and eight near misses were
played in the Mac's `say` voices through the Mac's speaker at the lamp, and
`lamp-name check` was run over what the lamp's second microphone heard. No
recording of it is kept, so here the same lines, and more, are said by the
Mac's own voices on this Mac, which the training never uses (Apple's voices
are not for training a model we ship; testing with them is all they do):

- "played": the six calls and eight near misses of that check, each said
  by every English voice `say` has here (eleven that differ: of the 29 it
  lists, the "Eddy" kind fall back to Samantha's or Daniel's sound, and
  Aman and Tara give silence until downloaded), each at its own speed and
  at 150 and 230 words a minute: 33 speakers. The two Vietnamese lines are
  said by its Vietnamese voice (Linh) at two speeds. Two of the lines were
  written there as "Lan, ..." and "Sam, ..."; what followed is not known,
  so a question follows here.
- "calls": "Lamp, ...", "Hey Lamp, ..." and "Lamp oi, ..." with eleven
  questions, by every English speaker; Linh says "Lamp ơi, ..." with
  Vietnamese questions, and reads the English calls as a Vietnamese voice
  would.
- "near": sixteen lines with a near miss where the name would be, by
  every English speaker; eight Vietnamese lines by Linh ("Lâm ơi", "Làm",
  "Lan ơi", "Nam ơi", ...).
- "talk": twelve LibriSpeech test sentences by each English voice, and 150
  FLEURS Vietnamese test sentences by Linh, for false hearings an hour.

Each line is heard as evaluate.py hears a call: after 2 s of quiet, at
-20 dBFS, followed by 1.5 s; "as said", and "played": the loudspeaker of
microphone.py at the middle of its ranges, a room, and the first 100 ms
after each silence 30 dB down. A line counts as heard when a hearing
begins from its start to 1 s after its end, by the rule of `said.rs`.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

import numpy as np
import soundfile as sf

import shared
from evaluate import Model, fixed_loudspeaker, fixed_onset_loss, seen_until
from features import FrontEnd, stacks
from hearing import hearings
from mix import RATE, active_level, gain_for, mixture
from store import Store, exists, save

THREADS = int(os.environ.get("OMP_NUM_THREADS", "6"))
NAME = "say-test"
LEASTS = [0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 0.99]
VOICE_DB = -20.0

# The English voices of `say` that sound different from each other on this Mac (macOS 26.6).
ENGLISH = ["Albert", "Daniel", "Fred", "Junior", "Karen", "Kathy", "Moira", "Ralph", "Rishi", "Samantha", "Tessa"]
# Each at its own speed and at 150 and 230 words a minute.
SPEAKERS = [(v, r) for v in ENGLISH for r in (None, 150, 230)]
VIETNAMESE = "Linh"

PLAYED_CALLS = [
    ("bare", "Lamp, tell me a short joke."), ("hey", "Hey Lamp, how are you?"), ("bare", "Lamp, are you awake?"),
    ("bare", "Lamp, what time is it?"), ("bare", "Lamp, what's the weather like?"), ("hey", "Hey Lamp, what day is it?"),
]
PLAYED_NEAR = [
    ("Lan", "Lan, what time is it?"), ("lamb", "Lamb is my favourite dish."), ("Sam", "Sam, are you coming?"),
    ("lamp in a sentence", "I left the lamp on in the kitchen."), ("land", "Land the plane carefully."),
    ("lab", "The lab is closed today."),
]
PLAYED_NEAR_VI = [("Lâm", "Lâm ơi, đi ăn trưa không?"), ("làm", "Làm xong chưa?")]

QUESTIONS = [
    "what time is it?", "tell me a joke.", "are you awake?", "how are you?", "can you help me?",
    "what's the weather like today?", "turn the light down a little.", "are you there?", "play some music.",
    "set a timer for ten minutes.", "what do you think?",
]
VI_QUESTIONS = ["mấy giờ rồi?", "kể chuyện cười đi.", "bạn có nghe không?", "hôm nay trời thế nào?",
                "bật nhạc lên đi.", "giúp mình với."]
NEAR = [
    ("Lan", "Lan, are you there?"), ("Lan", "Hey Lan, can you help me?"), ("Liam", "Liam, what time is it?"),
    ("lamb", "Lamb chops tonight?"), ("land", "Land ahead."), ("lab", "Lab results are in."),
    ("Sam", "Hey Sam, how are you?"), ("ma'am", "Ma'am, can you help me?"), ("plan", "Plan B, then."),
    ("rhymes", "Camp is over there."), ("rhymes", "Lamps are on sale."), ("rhymes", "Damp weather today."),
    ("lamp in a sentence", "Turn on the lamp."), ("lamp in a sentence", "This lamp is too bright."),
    ("lamb", "Lamb, please."), ("land", "Land, ho!"),
]
NEAR_VI = [
    ("Lâm", "Lâm ơi, mấy giờ rồi?"), ("Lâm", "Lâm, bạn rảnh không?"), ("làm", "Làm ơn nói chậm lại."),
    ("làm", "Làm bài tập chưa?"), ("Lan", "Lan ơi, đi đâu đó?"), ("other names", "Nam ơi, chờ mình với."),
    ("other names", "Em ơi, cho anh ly cà phê."), ("lắm", "Lắm việc quá."),
]


def lines() -> list[dict]:
    out = []
    for voice, rate in SPEAKERS:
        who = {"voice": voice, "rate": rate}
        for way, text in PLAYED_CALLS:
            out.append({"set": "played", "call": True, "way": way, **who, "text": text})
        for kind, text in PLAYED_NEAR:
            out.append({"set": "played", "call": False, "kind": kind, **who, "text": text})
        for q in QUESTIONS:
            for way, start in [("bare", "Lamp, "), ("hey", "Hey Lamp, "), ("oi", "Lamp oi, ")]:
                out.append({"set": "calls", "call": True, "way": way, **who, "text": start + q})
        for kind, text in NEAR:
            out.append({"set": "near", "call": False, "kind": kind, **who, "text": text})
    for rate in [175, 210]:
        for kind, text in PLAYED_NEAR_VI:
            out.append({"set": "played", "call": False, "kind": kind, "voice": VIETNAMESE, "text": text, "rate": rate})
        for q in VI_QUESTIONS:
            out.append({"set": "calls", "call": True, "way": "oi", "voice": VIETNAMESE, "text": "Lamp ơi, " + q, "rate": rate})
        for way, text in PLAYED_CALLS:
            out.append({"set": "calls", "call": True, "way": way, "voice": VIETNAMESE, "text": text, "rate": rate})
        for kind, text in NEAR_VI:
            out.append({"set": "near", "call": False, "kind": kind, "voice": VIETNAMESE, "text": text, "rate": rate})
    readers = Store("libri-talk-test")
    plain = [m["text"] for m in readers.meta if "lamp" not in m["text"]]
    rng = np.random.default_rng(5)
    for voice in ENGLISH:
        for k in rng.choice(len(plain), 12, replace=False):
            out.append({"set": "talk", "call": False, "kind": "talk", "voice": voice, "text": plain[int(k)]})
    fleurs = Store("fleurs-vi-test", shared.CLIPS)
    texts = sorted({m["text"] for m in fleurs.meta if "lâm" not in m["text"] and "làm" not in m["text"]})
    for k in np.random.default_rng(6).choice(len(texts), 150, replace=False):
        out.append({"set": "talk", "call": False, "kind": "talk", "voice": VIETNAMESE, "text": texts[int(k)]})
    return out


def say(voice: str, text: str, rate: int | None, path: Path) -> np.ndarray:
    command = ["say", "-v", voice, "-o", str(path), "--file-format=WAVE", "--data-format=LEI16@16000"]
    if rate:
        command += ["-r", str(rate)]
    subprocess.run(command + [text], check=True)
    x, r = sf.read(path, dtype="int16")
    assert r == RATE
    return x.astype(np.float32)


def make() -> None:
    if sys.platform != "darwin":
        raise SystemExit("the Mac's voices are needed")
    from concurrent.futures import ThreadPoolExecutor

    todo = lines()
    with tempfile.TemporaryDirectory() as tmp:

        def one(n: int) -> np.ndarray:
            m = todo[n]
            x = say(m["voice"], m["text"], m.get("rate"), Path(tmp) / f"line-{n}.wav")
            if n % 200 == 0:
                print(f"say: {n}/{len(todo)}", flush=True)
            # Leave out the quiet `say` puts before and after the line.
            loud = np.flatnonzero(np.abs(x) > 64)
            return x[max(0, loud[0] - 160) : loud[-1] + 160] if len(loud) else x

        with ThreadPoolExecutor(4) as pool:
            clips = list(pool.map(one, range(len(todo))))
    save(NAME, clips, todo, shared.CLIPS)


def room_fixed(x: np.ndarray) -> np.ndarray:
    """One room for every line: a decaying tail of 0.3 s, a fifth as loud as the direct sound."""
    from scipy.signal import fftconvolve

    rng = np.random.default_rng(3)
    n = int(0.3 * RATE)
    t = np.arange(n) / RATE
    tail = rng.standard_normal(n) * np.exp(-6.9 * t / 0.3)
    tail[: int(0.002 * RATE)] = 0.0
    h = tail / np.sqrt(np.sum(tail**2)) * 0.2
    h[0] = 1.0
    return fftconvolve(x, h)[: len(x)].astype(np.float32)


def heard_as(clip: np.ndarray, how: str) -> np.ndarray:
    lead, tail = np.zeros(2 * RATE, np.float32), np.zeros(int(1.5 * RATE), np.float32)
    x = np.concatenate([lead, clip * gain_for(active_level(clip), VOICE_DB), tail])
    if how == "played":
        x = room_fixed(fixed_loudspeaker(x))
        x = fixed_onset_loss(x * gain_for(active_level(x), VOICE_DB))
    return mixture(x, None)


def score(paths: list[str], every: int) -> None:
    front = FrontEnd(shared.STOP_CACHE / "oww", threads=THREADS)
    models = {Path(p).stem: Model(Path(p)) for p in paths}
    store = Store(NAME, shared.CLIPS)
    rows = []
    for i in range(0, len(store), every):
        m = store.meta[i]
        clip = store.audio(i)
        row = {k: m.get(k) for k in ("set", "call", "way", "kind", "voice", "rate", "text")}
        row["seconds"] = len(clip) / RATE
        for how in ["as said", "played"]:
            if m["set"] == "talk" and how == "played":
                continue
            x = stacks(front.of(heard_as(clip, how)))
            begin, last = 2.0, 2.0 + len(clip) / RATE + 1.0
            for name, model in models.items():
                s = model.scores(x)
                row[f"{name}|{how}|max"] = float(s.max()) if len(s) else 0.0
                for least in LEASTS:
                    found = [k for k in hearings(s, least, 1) if begin <= seen_until(k) <= last]
                    row[f"{name}|{how}|{least}"] = len(found)
        rows.append(row)
        if i % 500 < every:
            print(f"score: {i}/{len(store)}", flush=True)
    out = shared.CACHE / "runs" / ("played-" + "-".join(models) + (f"-every{every}" if every > 1 else "") + ".json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"models": list(models), "rows": rows}))
    print(f"wrote {out}")
    summary(out)


def summary(path: Path, leasts=(0.9, 0.98)) -> None:
    data = json.loads(Path(path).read_text())
    rows = data["rows"]
    for name in data["models"]:
        print(f"\n## {name}")
        for least in leasts:
            for how in ["as said", "played"]:
                key = f"{name}|{how}|{least}"
                groups = defaultdict(lambda: [0, 0])
                hours = 0.0
                for r in rows:
                    if key not in r:
                        continue
                    if r["set"] == "talk":
                        g = groups[("talk an hour", "English" if r["voice"] != VIETNAMESE else "Vietnamese")]
                        g[0] += r[key]
                        g[1] += r["seconds"]
                        continue
                    label = r["way"] if r["call"] else r["kind"]
                    who = "Linh" if r["voice"] == VIETNAMESE else "English voices"
                    g = groups[(r["set"], f"{label}, {who}")]
                    g[0] += int(r[key] > 0)
                    g[1] += 1
                print(f"\n{how}, {least}/1")
                for (s, label), (k, n) in sorted(groups.items()):
                    if s == "talk an hour":
                        print(f"  {s:12} {label:34} {k / (n / 3600):6.2f} ({k} in {n / 3600:.2f} h)")
                    else:
                        print(f"  {s:12} {label:34} {k:4}/{n:<4} ({100 * k / n:3.0f} %)")
                del hours


def verdict(path: Path, leasts=(0.8, 0.9, 0.95, 0.98, 0.99)) -> None:
    """The played check voice by voice: of the six calls, how many heard; of the eight near
    misses (six English lines in that voice, Linh's two at her usual speed), how many taken
    for the name; and the share of voices that would pass the first builder's bar (at least
    three calls of six, at most one near miss)."""
    data = json.loads(Path(path).read_text())
    rows = [r for r in data["rows"] if r["set"] == "played"]
    vietnamese = [r for r in rows if r["voice"] == VIETNAMESE]
    vietnamese = vietnamese[: len(vietnamese) // 2]  # the first speed, 175 words a minute
    print("model | as said or played | least | calls heard of 6 (mean) | near misses taken of 8 (mean) | voices passing")
    for name in data["models"]:
        for how in ["as said", "played"]:
            for least in leasts:
                key = f"{name}|{how}|{least}"
                vi_false = sum(int(r[key] > 0) for r in vietnamese)
                calls, false, passing = [], [], 0
                for voice, rate in SPEAKERS:
                    mine = [r for r in rows if r["voice"] == voice and r.get("rate") == rate]
                    c = sum(int(r[key] > 0) for r in mine if r["call"])
                    f = sum(int(r[key] > 0) for r in mine if not r["call"]) + vi_false
                    calls.append(c)
                    false.append(f)
                    passing += int(c >= 3 and f <= 1)
                print(f"{name} | {how} | {least} | {np.mean(calls):.1f} | {np.mean(false):.1f} | {passing}/{len(SPEAKERS)}")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("what", choices=["make", "score", "summary", "verdict"])
    p.add_argument("models", nargs="*")
    p.add_argument("--every", type=int, default=1)
    a = p.parse_args()
    if a.what == "make":
        if exists(NAME, shared.CLIPS):
            print(f"{NAME} is made already")
        else:
            make()
    elif a.what == "score":
        score(a.models, a.every)
    elif a.what == "summary":
        summary(Path(a.models[0]))
    else:
        verdict(Path(a.models[0]))
