"""Step 6 of make.sh: how the listener does on speakers it never heard.

    python evaluate.py MODEL.onnx [--every N] [--only catches,talk,ami,near]

Every number here is from recordings run through the front end and model
step by step, as `lamp-name` would run them, on whole streams of sound.

Catches. Each held-out target clip is put into 5 s of another voice, its
end at 3.2 s, at each relative level (the word's active level against the
other voice's; mix.py), with the other voice at -14 dBFS (the lamp's own
voice in its microphone, docs/duplex-listening.md). "Clean" is the word
alone at -20 dBFS. A catch is a hearing (hearing.py) that begins between
the target's start and 1 s after its end. The other voice is a LibriSpeech
test reader (real) or a held-out Piper voice (generated), and the same
stretch of it is used at every level. "Suppressed" is -10 dB with the
suppressor of mix.py at strength 1 and floor 0.1.

False alarms. Hearings an hour in long streams with no target in them:
LibriSpeech test readers (each utterance at -14 dBFS), held-out Piper
voices, and AMI meetings (ordinary talk; a hearing within 1.5 s after a
transcribed "stop", "wait" or "hold on" is counted apart). Near misses:
the share of clips with any hearing, each clip alone at -20 dBFS between
2 s of quiet before and 1.5 s after.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import zipfile
from collections import defaultdict
from pathlib import Path
from xml.etree import ElementTree

import numpy as np
import onnx
import onnxruntime as ort
import soundfile as sf

from features import FrontEnd, stacks
from hearing import hearings
from mix import FULL, RATE, active_level, gain_for, mixture, place, suppress
from store import CACHE, Store

THREADS = int(os.environ.get("OMP_NUM_THREADS", "6"))
LEVELS = ["clean", 0, -5, -10, -15, "suppressed"]
OTHER_DB = -14.0
CLEAN_DB = -20.0
LEASTS = [0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 0.99, 0.995]
RUNS = [1, 2]
# The score for input j is of the sound up to this many seconds.
FIRST_SEEN = 31_712 / RATE


class Model:
    """stop.onnx, with its batch made free so whole streams are scored at once (the same weights)."""

    def __init__(self, path: Path):
        m = onnx.load(str(path))
        m.graph.input[0].type.tensor_type.shape.dim[0].dim_param = "batch"
        m.graph.output[0].type.tensor_type.shape.dim[0].dim_param = "batch"
        options = ort.SessionOptions()
        options.intra_op_num_threads = THREADS
        self.session = ort.InferenceSession(m.SerializeToString(), options, providers=["CPUExecutionProvider"])

    def scores(self, x: np.ndarray) -> np.ndarray:
        if len(x) == 0:
            return np.zeros(0, np.float32)
        return self.session.run(None, {"input": x.astype(np.float32)})[0][:, 0]


def seen_until(j: np.ndarray | int):
    return FIRST_SEEN + 0.08 * np.asarray(j)


def at_level(x: np.ndarray, db: float) -> np.ndarray:
    return x * gain_for(active_level(x), db)


def stream_scores(front: FrontEnd, model: Model, sound: np.ndarray) -> np.ndarray:
    return model.scores(stacks(front.of(sound)))


def first_hearing_in(scores: np.ndarray, least: float, run: int, start: float, end: float):
    for i in hearings(scores, least, run):
        t = float(seen_until(i))
        if start <= t <= end:
            return t
    return None


def catches(front, model, every: int) -> list[dict]:
    """One row per target clip, other voice and level: its scores' hearings near the word."""
    sets = [
        ("sc-stop-test", "Speech Commands"),
        ("mswc-stop-test", "MSWC"),
        ("mswc-wait-test", "MSWC"),
        ("libri-asking-test", "LibriSpeech"),
        ("piper-asking-test", "Piper"),
    ]
    others = {
        "reader": Store("libri-talk-test"),
        "generated": Store("piper-talk-test"),
    }
    length = 5 * RATE
    rows = []
    for name, source in sets:
        store = Store(name)
        index = range(0, len(store), every)
        for i in index:
            meta = store.meta[i]
            target, start, end = meta["targets"][0]
            clip = store.audio(i)
            word = clip[int(start * RATE) : int(end * RATE)]
            gain_db = CLEAN_DB - active_level(word)
            at = int(round((3.2 - end) * RATE))
            person = place(clip * 10 ** (gain_db / 20), at, length)
            s, e = start + at / RATE, end + at / RATE
            for other_name, other_store in others.items():
                rng = np.random.default_rng([i, len(name), len(other_name)])
                parts, have = [], 0
                while have < length + RATE:
                    c = other_store.audio(int(rng.integers(len(other_store))))
                    parts += [at_level(c, OTHER_DB), np.zeros(int(0.2 * RATE), np.float32)]
                    have += len(c) + int(0.2 * RATE)
                other = np.concatenate(parts)
                o = int(rng.integers(0, len(other) - length))
                other = other[o : o + length]
                for level in LEVELS:
                    if level == "clean":
                        if other_name != "reader":
                            continue
                        sound = mixture(person, None)
                    elif level == "suppressed":
                        p = person * gain_for(CLEAN_DB, OTHER_DB - 10)
                        sound = suppress(p, other, strength=1.0, floor=0.1)
                        sound = mixture(sound, None)
                    else:
                        p = person * gain_for(CLEAN_DB, OTHER_DB + level)
                        sound = mixture(p, other)
                    scores = stream_scores(front, model, sound)
                    found = {}
                    for least in LEASTS:
                        for run in RUNS:
                            t = first_hearing_in(scores, least, run, s, e + 1.0)
                            found[f"{least}/{run}"] = None if t is None else round(t - e, 3)
                    # Hearings outside the word, in the other voice's 5 s.
                    outside = {
                        f"{least}/{run}": sum(1 for k in hearings(scores, least, run) if not (s <= seen_until(k) <= e + 1.0))
                        for least in LEASTS
                        for run in RUNS
                    }
                    rows.append(
                        {
                            "set": name, "source": source, "target": target, "text": meta.get("text", ""),
                            "speaker": meta.get("speaker", ""), "other": other_name, "level": level,
                            "found": found, "outside": outside, "peak": float(scores.max()) if len(scores) else 0.0,
                        }
                    )
        print(f"catches: {name} done ({len(rows)} rows)", flush=True)
    return rows


def count_hearings(scores: np.ndarray) -> dict:
    return {f"{least}/{run}": len(hearings(scores, least, run)) for least in LEASTS for run in RUNS}


def talk_streams(front, model, store: Store, every: int) -> dict:
    """Hearings an hour over the whole set played one clip after another, each at -14 dBFS, a speaker at a time."""
    total = defaultdict(int)
    seconds = 0.0
    groups = defaultdict(list)
    for i, m in enumerate(store.meta):
        groups[m.get("speaker", "")].append(i)
    keys = sorted(groups)[::every]
    for key in keys:
        parts = []
        for i in groups[key]:
            parts += [at_level(store.audio(i), OTHER_DB), np.zeros(int(0.3 * RATE), np.float32)]
        sound = mixture(np.concatenate(parts), None)
        scores = stream_scores(front, model, sound)
        for k, v in count_hearings(scores).items():
            total[k] += v
        seconds += len(sound) / RATE
    return {"hours": seconds / 3600, "hearings": dict(total)}


def ami_words(meeting: str):
    """All words said in a meeting (start, word), and each target said (target, start, end)."""
    path = CACHE / "downloads" / "ami" / "ami_public_manual_1.6.2.zip"
    words, targets = [], []
    with zipfile.ZipFile(path) as z:
        for name in z.namelist():
            if not re.fullmatch(rf"words/{meeting}\.[A-Z]\.words\.xml", name):
                continue
            root = ElementTree.fromstring(z.read(name))
            # One speaker's words in order: "hold" then "on" is only theirs.
            own = [
                (float(w.attrib["starttime"]), float(w.attrib.get("endtime", w.attrib["starttime"])), w.text.lower())
                for w in root.iter()
                if w.tag.endswith("w") and w.text and "starttime" in w.attrib and w.attrib.get("punc") != "true"
            ]
            words += [(s, w) for s, _, w in own]
            for k, (s, e, w) in enumerate(own):
                if w in ("stop", "wait"):
                    targets.append((w, s, e))
                elif w == "hold" and k + 1 < len(own) and own[k + 1][2] == "on":
                    targets.append(("hold on", s, own[k + 1][1]))
    return sorted(words), sorted(targets, key=lambda t: t[1])


def ami(front, model, every: int) -> dict:
    """Ordinary talk in meetings: hearings an hour away from any transcribed target, and the targets caught."""
    total = defaultdict(int)
    seconds = 0.0
    caught = defaultdict(lambda: defaultdict(int))
    said = defaultdict(int)
    details = []
    files = sorted((CACHE / "downloads" / "ami").glob("*.Mix-Headset.wav"))[::every]
    for path in files:
        meeting = path.name.split(".")[0]
        x, rate = sf.read(path, dtype="float32")
        assert rate == RATE
        x = x * FULL
        words, targets = ami_words(meeting)
        for target, _, _ in targets:
            said[target] += 1
        scores = stream_scores(front, model, x)
        for least in LEASTS:
            for run in RUNS:
                key = f"{least}/{run}"
                steps = hearings(scores, least, run)
                begun = [float(seen_until(k)) for k in steps]
                for target, s, e in targets:
                    caught[key][target] += any(s <= t <= e + 1.0 for t in begun)
                for k, t in zip(steps, begun):
                    near_target = any(s - 0.2 <= t <= e + 1.5 for _, s, e in targets)
                    if not near_target:
                        total[key] += 1
                    if least == 0.9 and run == 1:
                        context = " ".join(w for (a, w) in words if t - 2.5 <= a <= t + 0.2)
                        details.append({"meeting": meeting, "t": round(t, 2), "score": float(scores[k]), "target_said": near_target, "words": context})
        seconds += len(x) / RATE
        print(f"ami: {meeting} done", flush=True)
    return {
        "hours": seconds / 3600,
        "meetings": len(files),
        "hearings": dict(total),
        "targets_said": dict(said),
        "targets_caught": {k: dict(v) for k, v in caught.items()},
        "details_at_0.9": details,
    }


def near_misses(front, model, every: int) -> dict:
    """Share of near-miss clips with any hearing, by group, each clip alone."""
    out = {}
    jobs = [
        ("piper-near-test", "what"), ("piper-listening-test", "what"), ("mswc-near-test", "text"),
        ("sc-word-test", None), ("mswc-word-test", None),
    ]
    for name, key in jobs:
        store = Store(name)
        for i in range(0, len(store), every):
            meta = store.meta[i]
            group = f"{name.split('-')[0]}: {meta[key] if key else 'other words, one each'}"
            clip = at_level(store.audio(i), CLEAN_DB)
            sound = mixture(np.concatenate([np.zeros(2 * RATE, np.float32), clip, np.zeros(int(1.5 * RATE), np.float32)]), None)
            scores = stream_scores(front, model, sound)
            g = out.setdefault(group, {"clips": 0, "with_hearing": defaultdict(int)})
            g["clips"] += 1
            for k, v in count_hearings(scores).items():
                g["with_hearing"][k] += int(v > 0)
    # LibriSpeech test utterances that hold a near miss, whole.
    store = Store("libri-talk-test")
    for i, meta in list(enumerate(store.meta))[::every]:
        for word in meta.get("near", []):
            group = f"libri: {word}"
            clip = at_level(store.audio(i), CLEAN_DB)
            sound = mixture(np.concatenate([np.zeros(2 * RATE, np.float32), clip, np.zeros(int(1.5 * RATE), np.float32)]), None)
            scores = stream_scores(front, model, sound)
            g = out.setdefault(group, {"clips": 0, "with_hearing": defaultdict(int)})
            g["clips"] += 1
            for k, v in count_hearings(scores).items():
                g["with_hearing"][k] += int(v > 0)
    for g in out.values():
        g["with_hearing"] = dict(g["with_hearing"])
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("model")
    p.add_argument("--every", type=int, default=1, help="take one clip, speaker or meeting in N, for a quicker look")
    p.add_argument("--only", default="catches,talk,ami,near")
    args = p.parse_args()
    front = FrontEnd(CACHE / "oww", threads=THREADS)
    model = Model(Path(args.model))
    only = args.only.split(",")
    result = {"model": str(args.model), "every": args.every}
    if "catches" in only:
        result["catches"] = catches(front, model, args.every)
    if "talk" in only:
        result["talk_reader"] = talk_streams(front, model, Store("libri-talk-test"), args.every)
        print("talk: readers done", flush=True)
        result["talk_generated"] = talk_streams(front, model, Store("piper-talk-test"), args.every)
        print("talk: generated done", flush=True)
    if "ami" in only:
        result["ami"] = ami(front, model, args.every)
    if "near" in only:
        result["near"] = near_misses(front, model, args.every)
    runs = CACHE / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    every = f"-every{args.every}" if args.every > 1 else ""
    out = runs / f"{Path(args.model).stem}{every}-{'-'.join(only)}.eval.json"
    out.write_text(json.dumps(result))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
