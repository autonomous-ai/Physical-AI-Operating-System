"""Step 4 of make.sh: the training inputs, as `lamp-name` would give them to the model.

    python make_data.py words|talk|talk-more|check|check-talk --part I/K

Each example is a stretch of sound of 2.59 s, long enough for nine model
inputs 80 ms apart. Input j sees the sound up to (1280 j + 31712) / 16000 s
(features.py). Its label:

- 1 when a target ended 0.02 to 0.74 s before the end of what it sees;
- left out of training when a target is still being said at its end, or
  ended 0.74 to 1.2 s before (the word is in view, but old);
- 0 otherwise.

"words": each target clip put under another voice (or alone), and each
near miss, short reply or other word likewise, labelled 0.
"talk", "talk-more": long stretches of the other voice alone, and of two
voices, all 0.
"check", "check-talk": the same for validation: Speech Commands' validation
speakers under LibriSpeech readers held out of training, and an hour of
those readers alone.

LibriSpeech dev readers whose number ends in 3 are held out for validation.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import numpy as np

from features import STEP, WINDOW, FrontEnd, stacks
from mix import RATE, active_level, gain_for, mixture, place
from store import CACHE, Store

INPUTS = 9
LENGTH = WINDOW + STEP * (INPUTS - 1)
# The level noted for a word heard with no other voice.
ALONE = 99.0
SEEN_UNTIL = (STEP * np.arange(INPUTS) + 31_712) / RATE
OUT = CACHE / "inputs"


def labels(targets: list[tuple[float, float]]) -> np.ndarray:
    """Label of each input for targets (start, end) in seconds into the example: 1, 0, or -1 (left out)."""
    out = np.zeros(INPUTS, np.int8)
    for j, seen in enumerate(SEEN_UNTIL):
        for start, end in targets:
            since = seen - end
            if 0.02 <= since <= 0.74:
                out[j] = 1
                break
            if (since < 0.02 and seen > start) or 0.74 < since <= 1.2:
                out[j] = -1
    return out


def held_out_reader(meta: dict) -> bool:
    return meta.get("speaker", "").startswith("libri-") and meta["speaker"].endswith("3")


class Voices:
    """Stretches of another voice, taken from a set of clips one after another."""

    def __init__(self, store: Store, keep):
        self.store = store
        self.index = [i for i, m in enumerate(store.meta) if keep(m)]

    def stretch(self, rng: np.random.Generator, length: int) -> np.ndarray:
        parts, have = [], 0
        while have < length + RATE:
            clip = self.store.audio(self.index[rng.integers(len(self.index))])
            gap = np.zeros(int(rng.uniform(0.05, 0.4) * RATE), np.float32)
            parts += [clip, gap]
            have += len(clip) + len(gap)
        whole = np.concatenate(parts)
        at = rng.integers(0, len(whole) - length)
        return whole[at : at + length]


def scaled(x: np.ndarray, level_now: float, level_wanted: float) -> np.ndarray:
    return x * gain_for(level_now, level_wanted)


def sounding_end(clip: np.ndarray) -> float:
    """Seconds into a clip at which its last 10 ms louder than 35 dB under its loudest ends."""
    hop = RATE // 100
    n = len(clip) // hop
    power = np.mean(clip[: n * hop].reshape(n, hop).astype(np.float64) ** 2, axis=1) + 1e-9
    loud = np.flatnonzero(power >= power.max() * 10 ** (-35 / 10))
    return float((loud[-1] + 1) * hop / RATE) if len(loud) else len(clip) / RATE


def example_with(clip, targets, others, noise, rng, positive: bool, span=None):
    """One example: the clip under another voice (or alone), its sound and its labels.

    A target clip is placed so that a target ends 0.02 to 0.10 s before the
    first input's sound does. A short clip that holds no target is placed the
    same way by the end of its word (`span`) or of its sound, so that where
    a word sits says nothing of whether it is a target. A long one (talk) is
    placed anywhere.
    """
    if positive:
        start, end = targets[rng.integers(len(targets))]
    elif span is not None:
        start, end = span
    elif len(clip) < 3 * RATE:
        start, end = 0.0, sounding_end(clip)
    else:
        start = end = None
    if end is not None:
        anchor = SEEN_UNTIL[0] - rng.uniform(0.02, 0.10)
        at = int(round((anchor - end) * RATE))
    else:
        at = int(rng.integers(-len(clip) // 2, LENGTH - len(clip) // 2))
    person = place(clip, at, LENGTH)
    if targets:
        s, e = targets[0]
        word_level = active_level(clip[int(s * RATE) : int(e * RATE)])
    elif span is not None:
        word_level = active_level(clip[int(span[0] * RATE) : int(span[1] * RATE)])
    else:
        word_level = active_level(clip)
    other = None
    noise_part = None
    relative = ALONE
    if rng.random() < 0.12:
        person = scaled(person, word_level, rng.uniform(-35, -10))
    else:
        voices = others[0] if rng.random() < 0.6 else others[1]
        other = voices.stretch(rng, LENGTH)
        other_level = rng.uniform(-30, -10)
        other = scaled(other, active_level(other), other_level)
        relative = rng.uniform(-20, 6) if positive else rng.uniform(-20, 10)
        person = scaled(person, word_level, other_level + relative)
        if rng.random() < 0.1:
            n = noise.stretch(rng, LENGTH)
            noise_part = scaled(n, active_level(n), other_level - rng.uniform(5, 25))
    done = {}
    sound = mixture(person, other, rng, noise_part, done)
    shifted = [(s + at / RATE, e + at / RATE) for s, e in targets]
    y = labels(shifted) if positive else np.zeros(INPUTS, np.int8)
    return sound, y, relative, bool(done.get("suppressed"))


def words(part: int, parts: int, check: bool) -> None:
    front = FrontEnd(CACHE / "oww", threads=int(os.environ.get("OMP_NUM_THREADS", "2")))
    keep_train = lambda m: not held_out_reader(m)  # noqa: E731
    keep_check = held_out_reader
    libri = Store("libri-talk-train")
    piper_talk = Store("piper-talk-train")
    keep = keep_check if check else keep_train
    others = (Voices(libri, keep), Voices(piper_talk, lambda m: True))
    noise = Voices(Store("sc-noise"), lambda m: True)
    if check:
        positives = [("sc-stop-val", 1)]
        negatives = [("libri-talk-train", 1500)]
    else:
        positives = [
            ("sc-stop-train", 2), ("mswc-stop-train", 2), ("mswc-wait-train", 2),
            ("libri-asking-train", 3), ("piper-asking-train", 1),
        ]
        negatives = [
            ("piper-near-train", 9000), ("mswc-near-train", 6000), ("sc-word-train", 5000),
            ("mswc-word-train", 10000), ("piper-listening-train", 1200), ("libri-talk-train", 3000),
        ]
    jobs = []
    for name, repeat in positives:
        store = Store(name)
        for i, m in enumerate(store.meta):
            if (check or keep_train(m)) and m.get("targets"):
                jobs += [(name, i, r, True) for r in range(repeat)]
    for name, count in negatives:
        store = Store(name)
        index = [i for i, m in enumerate(store.meta) if keep(m) and not (name == "mswc-near-train" and m["text"] == "hold")]
        jobs += [(name, int(index[k % len(index)]), k // len(index), False) for k in range(count if not check else min(count, len(index)))]
    stores = {}
    xs, ys, srcs, levels, suppressed = [], [], [], [], []
    names = sorted({j[0] for j in jobs})

    for n, (name, i, r, positive) in enumerate(jobs):
        if n % parts != part:
            continue
        rng = np.random.default_rng([n, r, 7 if check else 1])
        store = stores.setdefault(name, Store(name))
        clip = store.audio(i)
        targets = [(t[1], t[2]) for t in store.meta[i].get("targets", [])]
        span = store.meta[i].get("span")
        sound, y, relative, was_suppressed = example_with(clip, targets, others, noise, rng, positive, span)
        x = stacks(front.of(sound))
        assert len(x) == INPUTS, (len(x), len(sound))
        keep_rows = y >= 0
        count = int(keep_rows.sum())
        xs.append(x[keep_rows].astype(np.float16))
        ys.append(y[keep_rows])
        srcs.append(np.full(count, names.index(name), np.int16))
        levels.append(np.full(count, relative, np.float16))
        suppressed.append(np.full(count, was_suppressed, np.int8))
        if n % 5000 < parts:
            print(f"part {part}: {n}/{len(jobs)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    tag = "check" if check else "words"
    np.savez(
        OUT / f"{tag}-{part}.npz",
        x=np.concatenate(xs), y=np.concatenate(ys), source=np.concatenate(srcs),
        level=np.concatenate(levels), suppressed=np.concatenate(suppressed), names=np.array(names),
    )
    print(f"{tag} part {part}: {sum(len(v) for v in ys)} inputs, {int(sum(v.sum() for v in ys))} positive")


def talk(part: int, parts: int, what: str) -> None:
    """Long stretches of the other voice alone, or of two voices, every input labelled 0.

    "talk": 10 h of single voices (LibriSpeech dev readers and generated)
    and 2 h of two at once. "talk-more": 21 h of train-clean-100's readers
    and 4 h of two at once. "check-talk": one hour of the held-out readers.
    """
    front = FrontEnd(CACHE / "oww", threads=int(os.environ.get("OMP_NUM_THREADS", "2")))
    check = what == "check-talk"
    libri = Voices(Store("libri-talk-train"), held_out_reader if check else (lambda m: not held_out_reader(m)))
    piper_talk = Voices(Store("piper-talk-train"), lambda m: True)
    minute = 60 * RATE
    if check:
        plan = [("one", libri)] * 60
    elif what == "talk-more":
        more = Voices(Store("libri-more-talk-train"), lambda m: True)
        plan = [("one", more)] * 1260 + [("two", more)] * 240
    else:
        plan = [("one", libri)] * 450 + [("one", piper_talk)] * 150 + [("two", libri)] * 120
    seed = {"check-talk": 4, "talk": 3, "talk-more": 5}[what]
    xs = []
    for n, (how, voices) in enumerate(plan):
        if n % parts != part:
            continue
        rng = np.random.default_rng([n, seed])
        a = voices.stretch(rng, minute)
        a = scaled(a, active_level(a), rng.uniform(-30, -10))
        b = None
        if how == "two":
            other = piper_talk if rng.random() < 0.5 else libri
            b = other.stretch(rng, minute)
            b = scaled(b, active_level(b), active_level(a) + rng.uniform(-20, 0))
        sound = mixture(a, b, rng) if b is not None else mixture(None, a, rng)
        xs.append(stacks(front.of(sound)).astype(np.float16))
        if n % 50 < parts:
            print(f"talk part {part}: {n}/{len(plan)}", flush=True)
    OUT.mkdir(parents=True, exist_ok=True)
    x = np.concatenate(xs)
    np.savez(OUT / f"{what}-{part}.npz", x=x, y=np.zeros(len(x), np.int8), source=np.full(len(x), -1, np.int16), names=np.array([]))
    print(f"{what} part {part}: {len(x)} inputs")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("what", choices=["words", "talk", "talk-more", "check", "check-talk"])
    p.add_argument("--part", default="0/1")
    a = p.parse_args()
    part, parts = (int(v) for v in a.part.split("/"))
    if a.what in ("talk", "talk-more", "check-talk"):
        talk(part, parts, a.what)
    else:
        words(part, parts, a.what == "check")
