"""Step 3 of make.sh: the name's own sets of clips, in lampOS/.cache/name/clips.

    python clips.py mswc|libri|fleurs|edacc|piper

- "mswc": MSWC English clips of "lamp" (a bare call, by real people) and of
  the near misses ("lamb", "land", "plan", "lab", "Sam", "Liam", "Lan",
  "lam", "madam", "hey", "lamps", rhymes), each aligned to its word and
  cut from 30 ms before it to a quarter second after: a call begins after
  a silence. Split by speaker as training/stop splits MSWC.
- "libri": LibriSpeech utterances with "lamp" inside them (not a call), from
  the sets training/stop made: dev and train-clean-100 to train, test to
  test.
- "fleurs": Google FLEURS Vietnamese, its own train and test splits.
- "edacc": EdAcc's Vietnamese speakers of English, for testing only.
- "piper": the generated speech of speak.py.
"""

from __future__ import annotations

import csv
import hashlib
import io
import json
import re
import sys
import tarfile
from collections import Counter, defaultdict

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

import shared
from store import Store, save

RATE = 16_000
CALL_WORDS = {"lamp"}
NEAR_WORDS = {
    "hey": "hey", "lamb": "lamb", "land": "land", "lands": "land", "plan": "plan", "plant": "plan",
    "lab": "lab", "lap": "rhymes", "sam": "Sam", "liam": "Liam", "lan": "Lan", "lam": "Lâm",
    "lamar": "other names", "madam": "ma'am", "camp": "rhymes", "damp": "rhymes", "ramp": "rhymes",
    "stamp": "rhymes", "clamp": "rhymes", "lamps": "rhymes",
}


def mswc_split(speaker: str) -> str:
    """As training/stop: "test" when the first byte of SHA-1 of the speaker's id is under 39."""
    return "test" if speaker and hashlib.sha1(speaker.encode()).digest()[0] < 39 else "train"


def to_int16(x: np.ndarray) -> np.ndarray:
    return np.clip(np.round(x * 32768.0), -32768, 32767).astype(np.int16)


def cut_call(x: np.ndarray, start: float, end: float):
    """From 30 ms before the word to 250 ms after, faded in over 10 ms and out over 20 ms."""
    a = max(0, int((start - 0.03) * RATE))
    b = min(len(x), int((end + 0.25) * RATE))
    y = np.array(x[a:b], np.float32)
    fade_in, fade_out = min(int(0.01 * RATE), len(y) // 2), min(int(0.02 * RATE), len(y) // 2)
    y[:fade_in] *= np.linspace(0, 1, fade_in, dtype=np.float32)
    y[len(y) - fade_out :] *= np.linspace(1, 0, fade_out, dtype=np.float32)
    return y, a / RATE


def mswc() -> None:
    from align import Aligner, faded

    wanted = CALL_WORDS | set(NEAR_WORDS)
    speaker_of = {}
    for split in ["train", "dev", "test"]:
        with open(shared.STOP_CACHE / "data" / "mswc_splits" / f"{split}.csv") as f:
            for row in csv.DictReader(f):
                if row["WORD"] in wanted:
                    speaker_of[row["LINK"]] = row["SPEAKER"]
    aligner = Aligner()
    sets = defaultdict(lambda: ([], []))
    why = Counter()
    for shard in sorted((shared.CACHE / "downloads" / "mswc").glob("*.tar.gz")):
        with tarfile.open(shard, "r|gz") as archive:
            for member in archive:
                name = member.name.strip("/").split("/")[-1]
                word, _, rest = name.partition("_common_voice_")
                if not member.isfile() or word not in wanted or not rest:
                    continue
                link = f"{word}/common_voice_{rest}"
                speaker = speaker_of.get(link, "")
                x, rate = sf.read(io.BytesIO(archive.extractfile(member).read()), dtype="float32")
                x = resample_poly(x, 1, rate // RATE).astype(np.float32) if rate != RATE else x
                words = aligner.words(x, word)
                if words is None or words[0].score < 0.2 or not (0.1 <= words[0].end - words[0].start <= 1.0):
                    why[f"{word}: poor alignment"] += 1
                    continue
                w = words[0]
                end = faded(x, w.start, w.end)
                y, at = cut_call(x, w.start, end)
                split = mswc_split(speaker)
                kind = "bare" if word in CALL_WORDS else NEAR_WORDS[word]
                group = "mswc-lamp" if word in CALL_WORDS else "mswc-near"
                clips, metas = sets[f"{group}-{split}"]
                clips.append(to_int16(y))
                metas.append({"id": link, "speaker": "mswc-" + (speaker[:16] or "unknown"), "text": word, "what": kind, "span": [w.start - at, end - at]})
                why[f"{word}: ok"] += 1
        print(f"{shard.name} read", flush=True)
    print("mswc:", dict(sorted(why.items())))
    for set_name, (clips, metas) in sorted(sets.items()):
        save(set_name, clips, metas, root=shared.CLIPS)


def libri() -> None:
    """LibriSpeech utterances in which "lamp" is said inside a sentence."""
    lamp = re.compile(r"\blamps?\b")
    for split, sources in [("train", ["libri-talk-train", "libri-more-talk-train"]), ("test", ["libri-talk-test"])]:
        clips, metas = [], []
        for source in sources:
            store = Store(source)
            for i, m in enumerate(store.meta):
                if lamp.search(m["text"]) and not (split == "train" and m["speaker"].endswith("3") and source == "libri-talk-train"):
                    clips.append(store.audio(i).astype(np.int16))
                    metas.append({**m, "what": "lamp in a sentence"})
        save(f"libri-lamp-{split}", clips, metas, root=shared.CLIPS)


def fleurs() -> None:
    """Vietnamese read speech, at 16 kHz."""
    folder = shared.CACHE / "downloads" / "fleurs"
    for split in ["train", "test"]:
        text_of = {}
        for line in (folder / f"{split}.tsv").read_text().splitlines():
            parts = line.split("\t")
            if len(parts) > 3:
                text_of[parts[1]] = (parts[0], parts[3])
        clips, metas = [], []
        with tarfile.open(folder / f"audio_{split}.tar.gz", "r|gz") as archive:
            for member in archive:
                name = member.name.split("/")[-1]
                if not member.isfile() or not name.endswith(".wav"):
                    continue
                # FLEURS keeps 32-bit floats: read as floats (read as 16-bit
                # integers, libsndfile gives silence).
                x, rate = sf.read(io.BytesIO(archive.extractfile(member).read()), dtype="float32")
                if rate != RATE:
                    x = resample_poly(x, RATE, rate).astype(np.float32)
                x = to_int16(x)
                sentence, text = text_of.get(name, ("", ""))
                clips.append(x)
                metas.append({"id": f"fleurs-vi/{split}/{name}", "sentence": sentence, "text": text, "speaker": "fleurs-vi"})
        save(f"fleurs-vi-{split}", clips, metas, root=shared.CLIPS)


def edacc() -> None:
    """EdAcc's test conversations, the Vietnamese speakers of English only: for testing, never training
    (CC BY-SA 4.0; whether share-alike would reach a model trained on it is unsettled)."""
    import pyarrow.parquet as pq

    clips, metas = [], []
    for path in sorted((shared.CACHE / "downloads" / "edacc").glob("test-*.parquet")):
        table = pq.read_table(path)
        for row in table.to_pylist():
            if "ietnam" not in (row.get("l1") or ""):
                continue
            x, rate = sf.read(io.BytesIO(row["audio"]["bytes"]), dtype="float32")
            if x.ndim > 1:
                x = x.mean(axis=1)
            if rate != RATE:
                x = resample_poly(x, RATE, rate).astype(np.float32)
            clips.append(to_int16(x))
            metas.append({"id": f"edacc/{row.get('speaker')}/{len(metas)}", "speaker": f"edacc-{row.get('speaker')}", "text": row.get("text", ""), "l1": row.get("l1")})
        print(f"{path.name} read", flush=True)
    save("edacc-vi-test", clips, metas, root=shared.CLIPS)


def piper() -> None:
    for kind in ["calls", "near", "replies", "talk"]:
        for split in ["train", "test"]:
            clips, metas = [], []
            for index in sorted((shared.CACHE / "made" / "piper").glob(f"{kind}-{split}*/index-*.jsonl")):
                folder = index.parent
                for line in index.read_text().splitlines():
                    if not line.strip():
                        continue
                    row = json.loads(line)
                    x, _ = sf.read(folder / row["path"], dtype="int16")
                    clips.append(x)
                    metas.append(row)
            if clips:
                name = "piper-accented-talk" if kind == "talk" else f"piper-{kind}"
                save(f"{name}-{split}", clips, metas, root=shared.CLIPS)


if __name__ == "__main__":
    for what in sys.argv[1:]:
        {"mswc": mswc, "libri": libri, "fleurs": fleurs, "edacc": edacc, "piper": piper}[what]()
