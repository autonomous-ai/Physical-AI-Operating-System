"""Test-oracle generator for lamp-endpoint parity; not used at runtime.

Inputs, all pinned:
  --features  Pipecat 53d313b176f65246e451282d5d03537574f54835
              src/pipecat/audio/turn/smart_turn/_whisper_features.py
  --model     smart-turn-v3.2-gpu.onnx (SHA256 ab8dc64b...539be)
Requires numpy and onnxruntime. Signals use integer xorshift32 arithmetic so
the Rust test regenerates identical PCM. Run with `python3 -I`.
"""

import argparse
import hashlib
import importlib.util
import json

import numpy as np
import onnxruntime as ort

STRIDE = 37


def xorshift(seed):
    state = seed
    while True:
        state ^= (state << 13) & 0xFFFFFFFF
        state ^= state >> 17
        state ^= (state << 5) & 0xFFFFFFFF
        yield state


def signals():
    rng = xorshift(1)
    noise = [next(rng) % 12001 - 6000 for _ in range(32000)]
    rng = xorshift(2)
    voiced = []
    for i in range(144000):
        pulse = 9000 if i % 100 < 3 else 0
        voiced.append(pulse * ((i // 1600) % 10 + 1) // 10 + next(rng) % 801 - 400)
    rng = xorshift(7)
    short = [next(rng) % 6001 - 3000 for _ in range(4800)]
    return {"noise_2s": noise, "voiced_9s": voiced, "short_0p3s": short, "silence": []}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--features", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("whisper_features", args.features)
    whisper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(whisper)
    with open(args.model, "rb") as handle:
        model_sha = hashlib.sha256(handle.read()).hexdigest()
    with open(args.features, "rb") as handle:
        features_sha = hashlib.sha256(handle.read()).hexdigest()
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    session = ort.InferenceSession(args.model, sess_options=options)
    cases = {}
    for name, pcm in signals().items():
        audio = np.asarray(pcm, dtype=np.int16).astype(np.float32) / 32768.0
        audio = audio[-128000:]
        audio = np.pad(audio, (128000 - audio.size, 0))
        features = whisper.compute_whisper_log_mel_features(audio, do_normalize=True)
        flat = features.astype(np.float64).ravel()
        probability = session.run(None, {"input_features": features[None]})[0].item()
        cases[name] = {
            "samples": len(pcm),
            "sum": float(flat.sum()),
            "abs_sum": float(np.abs(flat).sum()),
            "stride": STRIDE,
            "sampled": [float(v) for v in flat[::STRIDE]],
            "probability": probability,
        }
    reference = {
        "features_source_sha256": features_sha,
        "model_sha256": model_sha,
        "onnxruntime": ort.__version__,
        "numpy": np.__version__,
        "cases": cases,
    }
    with open(args.out, "w") as handle:
        json.dump(reference, handle, separators=(",", ":"))
        handle.write("\n")


if __name__ == "__main__":
    main()
