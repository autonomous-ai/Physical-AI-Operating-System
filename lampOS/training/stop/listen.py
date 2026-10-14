"""Run stop.onnx over files of sound, as `lamp-name` would, and say when it hears a stop.

    python listen.py MODEL.onnx FILE.wav... [--least 0.9] [--run 1] [--scores]

FILE is 16 kHz, one channel, 16 bits (WAV or raw), as `lamp-name check`
takes. For each file: every hearing (seconds into the file at which it
begins, and its score), then the highest score and when. With --scores,
every score, one a line, for comparing with another runner of the models.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path

import numpy as np
import onnxruntime as ort

from features import FrontEnd, score_time, stacks
from hearing import hearings
from store import CACHE


def samples_of(path: str) -> np.ndarray:
    data = Path(path).read_bytes()
    if data.startswith(b"RIFF") and len(data) >= 44:
        data = data[44:]
    return np.frombuffer(data[: len(data) // 2 * 2], "<i2").astype(np.float32)


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("model")
    p.add_argument("files", nargs="+")
    p.add_argument("--least", type=float, default=0.9)
    p.add_argument("--run", type=int, default=1)
    p.add_argument("--scores", action="store_true")
    a = p.parse_args()
    threads = int(os.environ.get("OMP_NUM_THREADS", "6"))
    front = FrontEnd(CACHE / "oww", threads=threads)
    model = ort.InferenceSession(a.model, providers=["CPUExecutionProvider"])
    for path in a.files:
        x = stacks(front.of(samples_of(path)))
        # One input at a time, the file's own shape [1, 16, 96], as on the lamp.
        scores = np.array([model.run(None, {"input": x[i : i + 1]})[0].reshape(-1)[0] for i in range(len(x))])
        if a.scores:
            for s in scores:
                print(f"{path}\t{s:.6f}")
            continue
        for i in hearings(scores, a.least, a.run):
            print(f"{path}\theard\t{score_time(i):.2f}\t{scores[i]:.4f}")
        if len(scores):
            top = int(np.argmax(scores))
            print(f"{path}\thighest\t{score_time(top):.2f}\t{scores[top]:.4f}")


if __name__ == "__main__":
    main()
