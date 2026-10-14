"""Generated speech for the name: calls, near misses, and the lamp's kind of replies.

    python speak.py calls|near|replies|talk --split train|test --count N --part I/K

The voice is Piper's `en_US-libritts-high` (904 speakers, trained from
scratch on LibriTTS), as for training/stop, with the same 113 speakers kept
for testing (the number 7 more than a multiple of 8).

A call or near miss is made in two pieces by the same voice: the start
("Hey Lamp", "Lamp", "Lan"...) and what follows, joined by a pause of 30
to 350 ms. So where the name ends is known without guessing. In a third of
them every word is read as espeak's Vietnamese voice would read it (the
phonemes given to the English voice): a stand-in for a Vietnamese speaker
of English, not a recording of one. "Replies" are whole lines in the
female voices only: a stand-in for the lamp's own voice (Gemini's "Kore"),
which cannot be used.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

import shared  # noqa: F401  (puts training/stop on the path)
from phrases import CALLS, ENDINGS, NEAR, QUESTIONS, REPLIES

VOICE = shared.STOP_CACHE / "piper" / "en_US-libritts-high.onnx"
# Two more voices, for testing only, both trained from scratch on public-domain
# recordings: a British English woman (Cori, LibriVox) and an American one
# (LJ Speech). They stand for other accents and, with the female LibriTTS
# voices, for the lamp's own voice.
OTHER_VOICES = {
    "cori": shared.CACHE / "piper" / "en_GB-cori-medium.onnx",
    "ljspeech": shared.CACHE / "piper" / "en_US-ljspeech-medium.onnx",
}
OUT = shared.CACHE / "made" / "piper"
RATE = 16_000


def speakers(split: str, female_only: bool = False) -> list[int]:
    who = [s for s in range(904) if (s % 8 == 7) == (split == "test")]
    if female_only:
        female = libritts_female()
        config = json.loads(VOICE.with_suffix(".onnx.json").read_text())
        number_of = {v: k.lstrip("p") for k, v in config["speaker_id_map"].items()}
        who = [s for s in who if number_of[s] in female]
    return who


def libritts_female() -> set[str]:
    path = shared.STOP_CACHE / "data" / "LibriSpeech" / "SPEAKERS.TXT"
    out = set()
    for line in path.read_text().splitlines():
        if line.startswith(";"):
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) >= 2 and parts[1] == "F":
            out.add(parts[0])
    return out


class Speaker:
    def __init__(self, threads: int, path=VOICE):
        import onnxruntime
        from piper import PiperVoice
        from piper.phonemize_espeak import EspeakPhonemizer

        self.voice = PiperVoice.load(str(path))
        options = onnxruntime.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        self.voice.session = onnxruntime.InferenceSession(str(path), sess_options=options, providers=["CPUExecutionProvider"])
        self.espeak = EspeakPhonemizer()
        self.known = set(self.voice.config.phoneme_id_map)

    def vietnamese_reading(self, text: str) -> str:
        """The line as espeak's Vietnamese voice reads it, given back as raw phonemes."""
        parts = re.split(r"(\[\[.*?\]\])", text)
        out = []
        for part in parts:
            if part.startswith("[["):
                out.append(part[2:-2].strip())
            elif part.strip():
                phonemes = "".join(sum(self.espeak.phonemize("vi", part), []))
                out.append("".join(c for c in phonemes if c in self.known))
        return "[[" + " ".join(out) + "]]"

    def say(self, text: str, speaker: int, rng: random.Random, accent: bool) -> np.ndarray:
        from piper import SynthesisConfig

        if accent:
            text = self.vietnamese_reading(text)
        config = SynthesisConfig(
            speaker_id=speaker,
            length_scale=rng.uniform(0.8, 1.3),
            noise_scale=rng.uniform(0.33, 0.8),
            noise_w_scale=rng.uniform(0.33, 0.9),
        )
        chunks = list(self.voice.synthesize(text, syn_config=config))
        if not chunks:
            return np.zeros(0, np.float32)
        audio = np.concatenate([c.audio_int16_array for c in chunks]).astype(np.float32) / 32768.0
        return resample_poly(audio, 320, 441).astype(np.float32)


def talk_lines(split: str) -> list[str]:
    """LibriSpeech transcripts without "lamp" (dev sets to train, test sets to test), 8 to 30 words."""
    sets = ["dev-clean", "dev-other"] if split == "train" else ["test-clean", "test-other"]
    out = []
    for s in sets:
        for path in sorted((shared.STOP_CACHE / "data" / "LibriSpeech" / s).rglob("*.trans.txt")):
            for line in path.read_text().splitlines():
                text = line.split(" ", 1)[1].lower()
                if not re.search(r"\blamps?\b", text) and 8 <= len(text.split()) <= 30:
                    out.append(text)
    return out


def trimmed(x: np.ndarray, floor_db: float = -40.0) -> np.ndarray:
    """Without the quiet at either end (10 ms frames under `floor_db` re the loudest)."""
    hop = RATE // 100
    n = len(x) // hop
    if n == 0:
        return x
    power = np.mean(x[: n * hop].reshape(n, hop) ** 2, axis=1) + 1e-12
    loud = np.flatnonzero(power >= power.max() * 10 ** (floor_db / 10))
    return x[loud[0] * hop : (loud[-1] + 1) * hop]


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("kind", choices=["calls", "near", "replies", "talk"])
    p.add_argument("--split", choices=["train", "test"], required=True)
    p.add_argument("--count", type=int, required=True)
    p.add_argument("--part", default="0/1")
    p.add_argument("--threads", type=int, default=2)
    p.add_argument("--voice", choices=["libritts", *OTHER_VOICES], default="libritts")
    a = p.parse_args()
    if a.voice != "libritts" and a.split != "test":
        raise SystemExit("the other voices are for testing only")
    part, parts = (int(x) for x in a.part.split("/"))
    os.environ["OMP_NUM_THREADS"] = str(a.threads)
    speaker = Speaker(a.threads, OTHER_VOICES.get(a.voice, VOICE))
    out = OUT / (f"{a.kind}-{a.split}" + ("" if a.voice == "libritts" else f"-{a.voice}"))
    out.mkdir(parents=True, exist_ok=True)
    index_path = out / f"index-{part}.jsonl"
    done = set()
    if index_path.exists():
        done = {json.loads(line)["path"] for line in index_path.read_text().splitlines() if line.strip()}
    index = open(index_path, "a")
    who = speakers(a.split, female_only=a.kind == "replies") if a.voice == "libritts" else [None]
    talk = talk_lines(a.split) if a.kind == "talk" else []
    made = 0
    for n in range(a.count):
        if n % parts != part or f"{n:06d}.wav" in done:
            continue
        rng = random.Random(f"name-{a.kind}-{a.split}-{n}")
        voice = rng.choice(who)
        row = {"path": f"{n:06d}.wav", "speaker": f"piper-{voice if voice is not None else a.voice}", "split": a.split}
        if a.kind == "replies":
            text = rng.choice(REPLIES)
            audio = trimmed(speaker.say(text, voice, rng, accent=False))
            row.update(text=text, what="reply")
        elif a.kind == "talk":
            # Ordinary sentences, three in four read as the Vietnamese voice
            # reads English: the accent of the calls, in talk that is not one.
            text = rng.choice(talk)
            accent = rng.random() < 0.75
            audio = trimmed(speaker.say(text, voice, rng, accent))
            row.update(text=text, what="talk", accent="vi" if accent else "en")
        else:
            if a.kind == "calls":
                what = rng.choice(list(CALLS))
                start = rng.choice(CALLS[what])
            else:
                what = rng.choice(list(NEAR))
                start = rng.choice(NEAR[what])
            accent = rng.random() < 1 / 3
            first = trimmed(speaker.say(start + rng.choice(ENDINGS), voice, rng, accent))
            # The name's end: the end of the first piece's sound.
            anchor = len(first) / RATE
            if what == "lamp in a sentence" or rng.random() < 0.25:
                audio, rest = first, ""
            else:
                rest = rng.choice(QUESTIONS)
                second = trimmed(speaker.say(rest[0].upper() + rest[1:], voice, rng, accent))
                pause = np.zeros(int(rng.uniform(0.03, 0.35) * RATE), np.float32)
                audio = np.concatenate([first, pause, second])
            reading = "vi" if accent else "en" if a.voice == "libritts" else a.voice
            row.update(text=f"{start}, {rest}".strip(", "), what=what, accent=reading, name_end=anchor)
        if len(audio) == 0:
            continue
        sf.write(out / row["path"], np.clip(audio * 32768, -32768, 32767).astype(np.int16), RATE)
        index.write(json.dumps(row) + "\n")
        index.flush()
        made += 1
    index.close()
    print(f"{a.kind} {a.split} part {part}/{parts}: {made} made", flush=True)


if __name__ == "__main__":
    main()
