"""A second voice generator: Kokoro-82M v1.0 (Apache-2.0 weights), run as ONNX.

Piper's LibriTTS voices are all one generator. Kokoro is another kind
(StyleTTS 2 with an iSTFTNet vocoder, not VITS), with 54 voices in eight
languages: a way to measure a name model on voices it never learned
from, and, tried, to train on more than one generator. Here:

- The text is turned into phonemes by the same espeak that Piper uses
  (piper's `EspeakPhonemizer`), with stress marks, and every symbol
  Kokoro does not know is dropped, as `kokoro-onnx` does.
- A voice is a style table (510 x 256, one row per length of the phoneme
  string). Training voices are blends of two voices at a random weight, so
  most training lines are said by a voice that does not exist on its own,
  as openWakeWord's training does with Piper's speakers.
- The non-English voices (Hindi, Japanese, Chinese, Spanish, French,
  Italian, Portuguese) read the English phonemes too: an accent of sorts.

Voices kept for testing only (`TEST_VOICES`) are never blended into a
training voice. `af_kore` is among them: it has the name of Gemini's
"Kore", the lamp's own voice; whether it sounds like it is not known.

The model in models/name is not trained on Kokoro at all: Kokoro is a
generator it never heard, for testing. Its training sets were tried
(make_data.py, NAME_KOKORO=on) and not kept.
"""

from __future__ import annotations

import json
import random
import re

import numpy as np
from scipy.signal import resample_poly

import shared

HERE = shared.CACHE / "kokoro"
MODEL = HERE / "onnx" / "model.onnx"
RATE = 16_000
SOURCE_RATE = 24_000

ALL_VOICES = sorted(p.stem for p in (HERE / "voices").glob("*.bin")) if (HERE / "voices").exists() else []
TEST_VOICES = {"af_kore", "af_bella", "am_michael", "bf_emma", "bm_george", "hf_beta", "zm_yunxi", "ef_dora", "pm_alex"}


def train_voices() -> list[str]:
    return [v for v in ALL_VOICES if v not in TEST_VOICES and v != "af"]


class KokoroSpeaker:
    def __init__(self, threads: int):
        import onnxruntime
        from piper.phonemize_espeak import EspeakPhonemizer

        options = onnxruntime.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        self.session = onnxruntime.InferenceSession(str(MODEL), sess_options=options, providers=["CPUExecutionProvider"])
        self.vocab = json.loads((HERE / "tokenizer.json").read_text())["model"]["vocab"]
        self.espeak = EspeakPhonemizer()
        self.styles: dict[str, np.ndarray] = {}

    def style(self, name: str) -> np.ndarray:
        if name not in self.styles:
            self.styles[name] = np.fromfile(HERE / "voices" / f"{name}.bin", dtype=np.float32).reshape(-1, 1, 256)
        return self.styles[name]

    def phonemes(self, text: str, language: str) -> str:
        """espeak's phonemes of `text` (raw phonemes inside [[ ]] are kept), only symbols Kokoro knows."""
        out = []
        for part in re.split(r"(\[\[.*?\]\])", text):
            if part.startswith("[["):
                out.append(part[2:-2].strip())
            elif part.strip():
                sentences = self.espeak.phonemize(language, part)
                out.append(" ".join("".join(s) for s in sentences))
        return "".join(c for c in " ".join(out) if c in self.vocab).strip()

    def say(self, text: str, voice: tuple[str, str, float], rng: random.Random, accent: bool) -> np.ndarray:
        """`voice` is (a, b, w): the style w * a + (1 - w) * b."""
        language = "vi" if accent else ("en" if voice[0].startswith("b") and rng.random() < 0.7 else "en-us")
        ids = [self.vocab[c] for c in self.phonemes(text, language)][:500]
        if not ids:
            return np.zeros(0, np.float32)
        a, b, w = voice
        style = w * self.style(a)[len(ids)] + (1 - w) * self.style(b)[len(ids)]
        audio = self.session.run(None, {
            "input_ids": np.array([[0, *ids, 0]], np.int64),
            "style": style.astype(np.float32),
            "speed": np.array([rng.uniform(0.8, 1.2)], np.float32),
        })[0].reshape(-1)
        return resample_poly(audio, 2, 3).astype(np.float32)


def pick_voice(split: str, rng: random.Random, female_only: bool = False) -> tuple[str, str, float]:
    """A test voice as it is, or a blend of two training voices."""
    if split == "test":
        pool = sorted(TEST_VOICES & set(ALL_VOICES))
        if female_only:
            pool = [v for v in pool if v[1] == "f"]
        v = rng.choice(pool)
        return (v, v, 1.0)
    pool = train_voices()
    if female_only:
        pool = [v for v in pool if v[1] == "f"]
    a, b = rng.choice(pool), rng.choice(pool)
    return (a, b, rng.uniform(0.0, 1.0))
