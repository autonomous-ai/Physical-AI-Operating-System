"""Step 3 of make.sh: the recordings of real people, cut and sorted into sets of clips.

    python clips.py sc|mswc|libri|piper|ami

Every set is split by speaker, never by clip, and a speaker in a "test" set
is in no "train" set:

- Speech Commands: its own lists (`validation_list.txt`, `testing_list.txt`),
  which put all of one speaker's clips in one list.
- MSWC: its splits share speakers, so here a speaker is "test" when the
  first byte of SHA-1 of its id is under 39 (about 15 %), across all three.
- LibriSpeech: dev-clean and dev-other to train; test-clean and test-other
  to test (different readers by construction).
- Piper: speakers 7, 15, 23, ... (see speak.py).

A clip that holds a target keeps where it is in `targets`: (target, start,
end) in seconds from the clip's start, the end where its sound fades.
"""

from __future__ import annotations

import csv
import hashlib
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

from phrases import has_target, near_words
from store import CACHE, save

DATA = CACHE / "data"
RATE = 16_000
NEAR_MSWC = ["hold", "holding", "stopped", "stopping", "stops", "waited", "waiting"]


def to_int16(x: np.ndarray) -> np.ndarray:
    return np.clip(np.round(x * 32768.0), -32768, 32767).astype(np.int16)


def cut(x: np.ndarray, start: float, end: float, margin: float = 0.25, fade: float = 0.02):
    """Samples from `margin` before `start` to `margin` after `end`, faded in and out; and the new start."""
    a = max(0, int((start - margin) * RATE))
    b = min(len(x), int((end + margin) * RATE))
    y = np.array(x[a:b], np.float32)
    n = min(int(fade * RATE), len(y) // 2)
    if n > 0:
        ramp = np.linspace(0.0, 1.0, n, dtype=np.float32)
        y[:n] *= ramp
        y[-n:] *= ramp[::-1]
    return y, a / RATE


def aligned_cuts(aligner, x: np.ndarray, text: str, least: float = 0.2):
    """Each target in a clip: the cut around it and its (target, start, end) within the cut."""
    from align import target_ends

    words = aligner.words(x, text)
    if words is None:
        return None, "not aligned"
    scores = {w.text: w.score for w in words}
    ends = target_ends(words, x)
    if not ends:
        return None, "no target"
    out = []
    for target, start, end in ends:
        if not (0.1 <= end - start <= 1.2):
            continue
        first = target.split()[0]
        if scores.get(first, 0.0) < least:
            continue
        y, at = cut(x, start, end)
        out.append((y, [target, start - at, end - at]))
    return out, ("ok" if out else "poor alignment")


def speech_commands() -> None:
    from align import Aligner

    root = DATA / "speech_commands"
    lists = {
        "val": set((root / "validation_list.txt").read_text().split()),
        "test": set((root / "testing_list.txt").read_text().split()),
    }

    def split_of(rel: str) -> str:
        return "test" if rel in lists["test"] else "val" if rel in lists["val"] else "train"

    aligner = Aligner()
    stop = defaultdict(lambda: ([], []))
    why = Counter()
    for path in sorted((root / "stop").glob("*.wav")):
        rel = f"stop/{path.name}"
        x, _ = sf.read(path, dtype="float32")
        found, reason = aligned_cuts(aligner, x, "stop")
        why[reason] += 1
        for y, target in found or []:
            clips, meta = stop[split_of(rel)]
            clips.append(to_int16(y))
            meta.append({"id": rel, "speaker": "sc-" + path.name.split("_nohash_")[0], "text": "stop", "targets": [target]})
    print("speech commands stop:", dict(why))
    for split, (clips, meta) in stop.items():
        save(f"sc-stop-{split}", clips, meta)

    # Other words, whole: ordinary short speech.
    other = defaultdict(lambda: ([], []))
    rng = np.random.default_rng(1)
    words = sorted(p.name for p in root.iterdir() if p.is_dir() and not p.name.startswith("_") and p.name != "stop")
    for word in words:
        files = sorted((root / word).glob("*.wav"))
        rng.shuffle(files)
        taken = Counter()
        for path in files:
            split = split_of(f"{word}/{path.name}")
            if split == "val" or taken[split] >= (250 if split == "train" else 60):
                continue
            taken[split] += 1
            x, _ = sf.read(path, dtype="int16")
            clips, meta = other[split]
            clips.append(x)
            meta.append({"id": f"{word}/{path.name}", "speaker": "sc-" + path.name.split("_nohash_")[0], "text": word})
    for split, (clips, meta) in other.items():
        save(f"sc-other-{split}", clips, meta)

    noise_clips, noise_meta = [], []
    for path in sorted((root / "_background_noise_").glob("*.wav")):
        x, _ = sf.read(path, dtype="int16")
        noise_clips.append(x)
        noise_meta.append({"id": path.name, "text": ""})
    save("sc-noise", noise_clips, noise_meta)


def mswc_split(speaker: str) -> str:
    return "test" if speaker and hashlib.sha1(speaker.encode()).digest()[0] < 39 else "train"


def mswc() -> None:
    from align import Aligner

    wanted = {"stop", "wait", *NEAR_MSWC}
    speaker_of = {}
    for split in ["train", "dev", "test"]:
        with open(DATA / "mswc_splits" / f"{split}.csv") as f:
            for row in csv.DictReader(f):
                if row["WORD"] in wanted:
                    speaker_of[row["LINK"]] = row["SPEAKER"]
    aligner = Aligner()
    sets = defaultdict(lambda: ([], []))
    why = Counter()
    for word in sorted(wanted):
        for path in sorted((DATA / "mswc" / word).glob("*.opus")):
            link = f"{word}/{path.name}"
            speaker = speaker_of.get(link, "")
            split = mswc_split(speaker)
            x, rate = sf.read(path, dtype="float32")
            x = resample_poly(x, 1, rate // RATE).astype(np.float32) if rate != RATE else x
            meta = {"id": link, "speaker": "mswc-" + (speaker[:16] or "unknown"), "text": word}
            if word in ("stop", "wait"):
                found, reason = aligned_cuts(aligner, x, word)
                why[f"{word} {reason}"] += 1
                for y, target in found or []:
                    clips, metas = sets[f"mswc-{word}-{split}"]
                    clips.append(to_int16(y))
                    metas.append({**meta, "targets": [target]})
            else:
                clips, metas = sets[f"mswc-near-{split}"]
                clips.append(to_int16(x))
                metas.append(meta)
    print("mswc:", dict(why))
    for name, (clips, metas) in sorted(sets.items()):
        save(name, clips, metas)


def librispeech() -> None:
    from align import Aligner

    aligner = Aligner()
    for split, parts in [("train", ["dev-clean", "dev-other"]), ("test", ["test-clean", "test-other"])]:
        talk, talk_meta, asking, asking_meta = [], [], [], []
        why = Counter()
        for part in parts:
            for trans in sorted((DATA / "LibriSpeech" / part).rglob("*.trans.txt")):
                for line in trans.read_text().splitlines():
                    uid, text = line.split(" ", 1)
                    x, _ = sf.read(trans.parent / f"{uid}.flac", dtype="float32")
                    meta = {"id": f"{part}/{uid}", "speaker": "libri-" + uid.split("-")[0], "text": text.lower()}
                    if not has_target(text):
                        talk.append(to_int16(x))
                        talk_meta.append({**meta, "near": near_words(text)})
                        continue
                    found, reason = aligned_cuts(aligner, x, text)
                    why[reason] += 1
                    for y, target in found or []:
                        asking.append(to_int16(y))
                        asking_meta.append({**meta, "targets": [target]})
        print(f"librispeech {split} utterances with a target:", dict(why))
        save(f"libri-talk-{split}", talk, talk_meta)
        save(f"libri-asking-{split}", asking, asking_meta)


def piper() -> None:
    """The generated speech of speak.py, gathered into sets."""
    for kind in ["asking", "near", "listening", "talk"]:
        for split in ["train", "test"]:
            folder = CACHE / "made" / "piper" / f"{kind}-{split}"
            clips, metas = [], []
            for index in sorted(folder.glob("index-*.jsonl")):
                for line in index.read_text().splitlines():
                    row = json.loads(line)
                    x, _ = sf.read(folder / row["path"], dtype="int16")
                    clips.append(x)
                    metas.append(row)
            if clips:
                save(f"piper-{kind}-{split}", clips, metas)


if __name__ == "__main__":
    for what in sys.argv[1:]:
        {"sc": speech_commands, "mswc": mswc, "libri": librispeech, "piper": piper}[what]()
