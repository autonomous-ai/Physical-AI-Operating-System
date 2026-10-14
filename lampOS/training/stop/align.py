"""Where each word lies in a recording, from what was said.

wav2vec 2.0 base fine-tuned on LibriSpeech 960 h (torchaudio's
`WAV2VEC2_ASR_BASE_960H`, MIT licence) is forced to the known words with
CTC alignment. It is used only to cut and place words here; nothing of it
goes into the lamp's model.
"""

from __future__ import annotations

import os
import re
from dataclasses import dataclass
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
CACHE = HERE.parent.parent / ".cache" / "stop"
os.environ.setdefault("TORCH_HOME", str(CACHE / "torch"))

import torch  # noqa: E402
import torchaudio  # noqa: E402
import torchaudio.functional as F  # noqa: E402


@dataclass
class Word:
    text: str
    start: float  # seconds
    end: float
    score: float  # mean probability of its letters


class Aligner:
    def __init__(self, threads: int | None = None):
        torch.set_num_threads(threads or int(os.environ.get("OMP_NUM_THREADS", "6")))
        bundle = torchaudio.pipelines.WAV2VEC2_ASR_BASE_960H
        self.model = bundle.get_model().eval()
        self.labels = bundle.get_labels()
        self.index = {c: i for i, c in enumerate(self.labels)}
        self.rate = bundle.sample_rate

    def words(self, samples: np.ndarray, text: str) -> list[Word] | None:
        """Word times in `samples` (16 kHz, float -1..1) for `text`, or None if it cannot be aligned."""
        words = normalise(text).split()
        if not words:
            return None
        tokens = [self.index[c] for w in words for c in w]
        with torch.inference_mode():
            emissions, _ = self.model(torch.from_numpy(np.asarray(samples, np.float32))[None])
            log_probs = torch.log_softmax(emissions, dim=-1)
        frames = log_probs.shape[1]
        if frames < len(tokens):
            return None
        try:
            aligned, scores = F.forced_align(log_probs, torch.tensor([tokens], dtype=torch.int32), blank=0)
        except RuntimeError:
            return None
        spans = F.merge_tokens(aligned[0], scores[0].exp())
        seconds = len(samples) / self.rate / frames
        out = []
        at = 0
        for w in words:
            letters = spans[at : at + len(w)]
            at += len(w)
            out.append(
                Word(
                    w.lower(),
                    letters[0].start * seconds,
                    letters[-1].end * seconds,
                    float(np.mean([s.score for s in letters])),
                )
            )
        return out


def target_ends(words: list[Word], samples: np.ndarray, rate: int = 16_000) -> list[tuple[str, float, float]]:
    """Each target said in `words`: (target, start, end in seconds), the end moved on to where its sound fades.

    Alignment puts a word's end at its last letter's frame, which can be
    before the sound of a closing "p" or "t" has died away.
    """
    found = []
    for i, w in enumerate(words):
        if w.text in ("stop", "wait"):
            found.append((w.text, w.start, faded(samples, w.start, w.end, rate)))
        elif w.text == "hold" and i + 1 < len(words) and words[i + 1].text == "on":
            found.append(("hold on", w.start, faded(samples, w.start, words[i + 1].end, rate)))
    return found


def faded(samples: np.ndarray, start: float, end: float, rate: int = 16_000, reach: float = 0.15) -> float:
    """The time after `end`, within `reach` s, at which the word's sound is 25 dB under its loudest."""
    hop = rate // 100
    x = np.asarray(samples, np.float64)
    a, b, c = int(start * rate), int(end * rate), min(len(x), int((end + reach) * rate))
    if b <= a:
        return end
    energy = lambda s, e: 10 * np.log10(np.mean(x[s:e] ** 2) + 1e-12)  # noqa: E731
    loudest = max(energy(i, i + hop) for i in range(a, max(a + 1, b - hop + 1), hop))
    at = b
    while at + hop <= c and energy(at, at + hop) > loudest - 25:
        at += hop
    return at / rate


def normalise(text: str) -> str:
    """Upper case letters and apostrophes only, the alphabet of the aligner."""
    text = text.upper().replace("-", " ")
    text = re.sub(r"[^A-Z' ]", " ", text)
    return re.sub(r"\s+", " ", text).strip()
