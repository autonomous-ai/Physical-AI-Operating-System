"""Step 6 of make.sh: how a name model does on voices it never heard.

    python evaluate.py MODEL.onnx [--every N] [--only calls,near,talk,ami]

Every number comes from streams of sound run through the front end and the
model step by step, as `lamp-name` runs them, and the rule of `said.rs`
(training/stop/hearing.py: a hearing begins at the first step at or over
the threshold, or N steps in a row).

- Calls: each held-out call put after 2 s of quiet and followed by 1.5 s,
  the voice at -20 dBFS, in five conditions: as recorded; the first 100 ms
  after each silence taken down by 30 dB ("onset lost"); through the
  loudspeaker of microphone.py with fixed settings; both; and with a
  LibriSpeech test reader talking 15 dB under it. A catch is a hearing that
  begins from the start of the line to 1 s after the name ends.
- Near misses: the share of clips with any hearing, as recorded and with
  both the lamp's microphone effects, by kind.
- Talk: hearings an hour in LibriSpeech test readers (each utterance at
  -20 dBFS, one after another), in the lamp-like replies of held-out
  generated female voices (the stand-in for the lamp's own voice), in
  FLEURS Vietnamese test speech, in held-out generated talk read with a
  Vietnamese accent, in EdAcc's Vietnamese speakers of English
  (when it has been fetched), and in 34 AMI meetings as recorded.
"""

from __future__ import annotations

import argparse
import json
import os
from collections import defaultdict
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
import soundfile as sf

import shared
from features import FrontEnd, stacks
from hearing import hearings
from microphone import clip_onsets, loudspeaker, onsets
from mix import FULL, RATE, active_level, gain_for, mixture
from store import Store, exists

THREADS = int(os.environ.get("OMP_NUM_THREADS", "6"))
LEASTS = [0.5, 0.7, 0.8, 0.9, 0.95, 0.98, 0.99, 0.995, 0.999]
RUNS = [1, 2]
FIRST_SEEN = 31_712 / RATE
VOICE_DB = -20.0
CONDITIONS = ["as recorded", "onset lost", "loudspeaker", "both", "talk around"]


class Model:
    """The model with its batch made free, so a whole stream is scored at once (the same weights)."""

    def __init__(self, path: Path):
        m = onnx.load(str(path))
        m.graph.input[0].type.tensor_type.shape.dim[0].dim_param = "batch"
        m.graph.output[0].type.tensor_type.shape.dim[0].dim_param = "batch"
        options = ort.SessionOptions()
        options.intra_op_num_threads = THREADS
        self.session = ort.InferenceSession(m.SerializeToString(), options, providers=["CPUExecutionProvider"])
        self.input = self.session.get_inputs()[0].name

    def scores(self, x: np.ndarray) -> np.ndarray:
        if len(x) == 0:
            return np.zeros(0, np.float32)
        return self.session.run(None, {self.input: x.astype(np.float32)})[0].reshape(-1)


def seen_until(j):
    return FIRST_SEEN + 0.08 * np.asarray(j)


def fixed_onset_loss(x: np.ndarray) -> np.ndarray:
    y = np.array(x, np.float32)
    for at in onsets(y):
        end = min(len(y), at + int(0.1 * RATE))
        y[at:end] *= 10 ** (-30 / 20)
    return y


def fixed_loudspeaker(x: np.ndarray) -> np.ndarray:
    class Fixed:
        """The middle of microphone.py's ranges, every time."""

        values = iter([425.0, 4500.0, 1650.0, 5.5, 1.75])

        def uniform(self, a, b):
            return next(self.values)

    return loudspeaker(x, Fixed())


def condition(clip: np.ndarray, how: str, rng: np.random.Generator, talk: Store | None) -> np.ndarray:
    lead, tail = np.zeros(2 * RATE, np.float32), np.zeros(int(1.5 * RATE), np.float32)
    x = np.concatenate([lead, clip * gain_for(active_level(clip), VOICE_DB), tail])
    if how in ("loudspeaker", "both"):
        x = fixed_loudspeaker(x)
        x = x * gain_for(active_level(x), VOICE_DB)
    if how in ("onset lost", "both"):
        x = fixed_onset_loss(x)
    other = None
    if how == "talk around":
        parts, have = [], 0
        while have < len(x):
            c = talk.audio(int(rng.integers(len(talk))))
            parts += [c * gain_for(active_level(c), VOICE_DB - 15), np.zeros(int(0.2 * RATE), np.float32)]
            have += len(c) + int(0.2 * RATE)
        other = np.concatenate(parts)[: len(x)]
    return mixture(x, other)


def scores_of(front, model, sound):
    return model.scores(stacks(front.of(sound)))


def calls(front, model, every: int) -> list[dict]:
    talk = Store("libri-talk-test")
    rows = []
    for name in ["piper-calls-test", "mswc-lamp-test"]:
        store = Store(name, shared.CLIPS)
        for i in range(0, len(store), every):
            meta = store.meta[i]
            clip = store.audio(i)
            end = meta.get("name_end", meta.get("span", [0, 0])[1])
            rng = np.random.default_rng([i, len(name)])
            found = {}
            for how in CONDITIONS:
                s = scores_of(front, model, condition(clip, how, rng, talk))
                begin, last = 2.0, 2.0 + end + 1.0
                for least in LEASTS:
                    for run in RUNS:
                        found[f"{how}|{least}/{run}"] = any(begin <= seen_until(k) <= last for k in hearings(s, least, run))
            rows.append({
                "set": name, "way": meta.get("what", "bare"), "accent": meta.get("accent", "real"),
                "text": meta.get("text", ""), "speaker": meta.get("speaker", ""), "found": found,
            })
        print(f"calls: {name} done", flush=True)
    return rows


def near(front, model, every: int) -> dict:
    out = {}
    jobs = [("piper-near-test", shared.CLIPS, "generated"), ("mswc-near-test", shared.CLIPS, "real"),
            ("libri-lamp-test", shared.CLIPS, "real"), ("piper-replies-test", shared.CLIPS, "generated")]
    for name, root, source in jobs:
        store = Store(name, root)
        for i in range(0, len(store), every):
            meta = store.meta[i]
            kind = meta.get("what", "lamp in a sentence")
            if kind == "reply":
                kind = "the lamp's own replies"
            group = f"{kind} ({source})"
            g = out.setdefault(group, {"clips": 0, "with_hearing": defaultdict(int)})
            g["clips"] += 1
            rng = np.random.default_rng(i)
            for how in ["as recorded", "both"]:
                s = scores_of(front, model, condition(store.audio(i), how, rng, None))
                for least in LEASTS:
                    for run in RUNS:
                        g["with_hearing"][f"{how}|{least}/{run}"] += int(len(hearings(s, least, run)) > 0)
        print(f"near: {name} done", flush=True)
    for g in out.values():
        g["with_hearing"] = dict(g["with_hearing"])
    return out


def talk_streams(front, model, store: Store, every: int, by_speaker: bool = True) -> dict:
    total = defaultdict(int)
    seconds = 0.0
    groups = defaultdict(list)
    for i, m in enumerate(store.meta):
        groups[m.get("speaker", "") if by_speaker else i // 40].append(i)
    for key in sorted(groups)[::every]:
        parts = []
        for i in groups[key]:
            c = store.audio(i)
            parts += [c * gain_for(active_level(c), VOICE_DB), np.zeros(int(0.3 * RATE), np.float32)]
        sound = mixture(np.concatenate(parts), None)
        s = scores_of(front, model, sound)
        for least in LEASTS:
            for run in RUNS:
                total[f"{least}/{run}"] += len(hearings(s, least, run))
        seconds += len(sound) / RATE
    return {"hours": seconds / 3600, "hearings": dict(total)}


def ami(front, model, every: int) -> dict:
    total = defaultdict(int)
    seconds = 0.0
    files = sorted((shared.STOP_CACHE / "downloads" / "ami").glob("*.Mix-Headset.wav"))[::every]
    for path in files:
        x, rate = sf.read(path, dtype="float32")
        assert rate == RATE
        s = scores_of(front, model, x * FULL)
        for least in LEASTS:
            for run in RUNS:
                total[f"{least}/{run}"] += len(hearings(s, least, run))
        seconds += len(x) / RATE
        print(f"ami: {path.name} done", flush=True)
    return {"hours": seconds / 3600, "meetings": len(files), "hearings": dict(total)}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("model")
    p.add_argument("--every", type=int, default=1)
    p.add_argument("--only", default="calls,near,talk,ami", help="any of calls, near, talk, ami, edacc")
    a = p.parse_args()
    front = FrontEnd(shared.STOP_CACHE / "oww", threads=THREADS)
    model = Model(Path(a.model))
    only = a.only.split(",")
    result = {"model": a.model, "every": a.every}
    if "calls" in only:
        result["calls"] = calls(front, model, a.every)
    if "near" in only:
        result["near"] = near(front, model, a.every)
    if "talk" in only:
        result["talk"] = {
            "LibriSpeech test readers": talk_streams(front, model, Store("libri-talk-test"), a.every),
            "the lamp's kind of replies, generated": talk_streams(front, model, Store("piper-replies-test", shared.CLIPS), a.every),
            "FLEURS Vietnamese": talk_streams(front, model, Store("fleurs-vi-test", shared.CLIPS), a.every, by_speaker=False),
            "generated talk, Vietnamese accent": talk_streams(front, model, Store("piper-accented-talk-test", shared.CLIPS), a.every),
        }
        print("talk done", flush=True)
    if "edacc" in only or ("talk" in only and exists("edacc-vi-test", shared.CLIPS)):
        result["talk_edacc"] = {"EdAcc, Vietnamese speakers of English": talk_streams(front, model, Store("edacc-vi-test", shared.CLIPS), a.every)}
    if "ami" in only:
        result["ami"] = ami(front, model, max(1, a.every))
    runs = shared.CACHE / "runs"
    runs.mkdir(parents=True, exist_ok=True)
    every = f"-every{a.every}" if a.every > 1 else ""
    out = runs / f"{Path(a.model).stem}{every}-{'-'.join(only)}.eval.json"
    out.write_text(json.dumps(result))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
