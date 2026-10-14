"""A set of clips on disk: their samples (16 kHz, 16 bits) laid end to end, and what each is.

Sets live in lampOS/.cache/stop/clips unless another folder is given
(training/name keeps its own sets in lampOS/.cache/name/clips and reads
the shared ones from here).
"""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
CACHE = HERE.parent.parent / ".cache" / "stop"
CLIPS = CACHE / "clips"


def save(name: str, clips: list[np.ndarray], meta: list[dict], root: Path = CLIPS) -> None:
    assert len(clips) == len(meta)
    root.mkdir(parents=True, exist_ok=True)
    offsets = np.zeros(len(clips) + 1, np.int64)
    offsets[1:] = np.cumsum([len(c) for c in clips])
    audio = np.concatenate(clips).astype(np.int16) if clips else np.zeros(0, np.int16)
    np.save(root / f"{name}.audio.npy", audio)
    np.save(root / f"{name}.offsets.npy", offsets)
    (root / f"{name}.json").write_text("\n".join(json.dumps(m) for m in meta) + "\n")
    hours = len(audio) / 16_000 / 3600
    print(f"{name}: {len(clips)} clips, {hours:.2f} h")


class Store:
    def __init__(self, name: str, root: Path = CLIPS):
        self.name = name
        self.samples = np.load(root / f"{name}.audio.npy", mmap_mode="r")
        self.offsets = np.load(root / f"{name}.offsets.npy")
        self.meta = [json.loads(line) for line in (root / f"{name}.json").read_text().splitlines() if line]

    def __len__(self) -> int:
        return len(self.meta)

    def audio(self, i: int) -> np.ndarray:
        """Clip i, as floats on the scale of 16-bit sound."""
        return np.asarray(self.samples[self.offsets[i] : self.offsets[i + 1]], np.float32)

    def seconds(self) -> float:
        return len(self.samples) / 16_000


def exists(name: str, root: Path = CLIPS) -> bool:
    return (root / f"{name}.json").exists()
