"""Generated speech: the targets, the near misses and long talk, in many voices.

The voice is Piper's `en_US-libritts-high` (904 speakers), trained from
scratch on LibriTTS train-clean-360 (CC BY 4.0). Not `libritts_r-medium`,
which openWakeWord uses: that one was fine-tuned from the Lessac voice,
whose data is licensed for research only.

Speakers whose number is 7 more than a multiple of 8 (113 of 904) are kept
for testing and never trained on.

    python speak.py asking|near|listening|talk --split train|test --count N --part I/K
"""

from __future__ import annotations

import argparse
import json
import os
import random
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

from phrases import ASKING, LISTENING, NEAR

HERE = Path(__file__).resolve().parent
CACHE = HERE.parent.parent / ".cache" / "stop"
VOICE = CACHE / "piper" / "en_US-libritts-high.onnx"
OUT = CACHE / "made" / "piper"


def speakers(split: str) -> list[int]:
    return [s for s in range(904) if (s % 8 == 7) == (split == "test")]


def lines(kind: str, rng: random.Random) -> tuple[str, str]:
    """A line of the kind asked for, and what it is (its target or near-miss group)."""
    if kind == "asking":
        target = rng.choice(list(ASKING))
        return rng.choice(ASKING[target]), target
    if kind == "near":
        group = rng.choice(list(NEAR))
        return rng.choice(NEAR[group]), group
    if kind == "listening":
        return rng.choice(LISTENING), "listening"
    raise ValueError(kind)


def talk_lines(split: str) -> list[str]:
    """Sentences for long talk: LibriSpeech transcripts holding no target (dev sets to train, test sets to test)."""
    from phrases import has_target

    sets = ["dev-clean", "dev-other"] if split == "train" else ["test-clean", "test-other"]
    out = []
    for s in sets:
        for path in sorted((CACHE / "data" / "LibriSpeech" / s).rglob("*.trans.txt")):
            for line in path.read_text().splitlines():
                text = line.split(" ", 1)[1].lower()
                if not has_target(text) and 8 <= len(text.split()) <= 40:
                    out.append(text)
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("kind", choices=["asking", "near", "listening", "talk"])
    p.add_argument("--split", choices=["train", "test"], required=True)
    p.add_argument("--count", type=int, required=True)
    p.add_argument("--part", default="0/1")
    p.add_argument("--threads", type=int, default=2)
    args = p.parse_args()
    part, parts = (int(x) for x in args.part.split("/"))

    os.environ["OMP_NUM_THREADS"] = str(args.threads)
    from piper import PiperVoice, SynthesisConfig

    voice = PiperVoice.load(str(VOICE))
    if args.kind == "asking":
        from align import Aligner, target_ends

        aligner = Aligner(threads=args.threads)
    out = OUT / f"{args.kind}-{args.split}"
    out.mkdir(parents=True, exist_ok=True)
    index = open(out / f"index-{part}.jsonl", "w")
    who = speakers(args.split)
    talk = talk_lines(args.split) if args.kind == "talk" else None
    made = 0
    for n in range(args.count):
        if n % parts != part:
            continue
        rng = random.Random(f"{args.kind}-{args.split}-{n}")
        if talk is not None:
            text, what = rng.choice(talk), "talk"
        else:
            text, what = lines(args.kind, rng)
        speaker = rng.choice(who)
        config = SynthesisConfig(
            speaker_id=speaker,
            length_scale=rng.uniform(0.8, 1.3),
            noise_scale=rng.uniform(0.33, 0.8),
            noise_w_scale=rng.uniform(0.33, 0.9),
        )
        audio = np.concatenate([c.audio_int16_array for c in voice.synthesize(text, syn_config=config)])
        audio = resample_poly(audio.astype(np.float32) / 32768.0, 320, 441).astype(np.float32)
        row = {"path": f"{n:06d}.wav", "text": text, "what": what, "speaker": f"piper-{speaker}", "split": args.split}
        if args.kind == "asking":
            words = aligner.words(audio, text)
            if words is None:
                continue
            ends = target_ends(words, audio)
            if not ends:
                continue
            row["targets"] = ends
        sf.write(out / row["path"], np.clip(audio * 32768, -32768, 32767).astype(np.int16), 16_000)
        index.write(json.dumps(row) + "\n")
        made += 1
    index.close()
    print(f"{args.kind} {args.split} part {part}/{parts}: {made} made")


if __name__ == "__main__":
    main()
