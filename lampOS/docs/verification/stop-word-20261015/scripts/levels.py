"""Output level of each suppressor setting in each played word's window and
in the lamp's speech away from the words (dBFS), and how much the outputs differ."""
import math, os, sys, wave
import numpy as np
cache, data = sys.argv[1], sys.argv[2]
MODES = ["standard", "near-end", "transparent"]
def wav(path):
    with wave.open(path) as w:
        return np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(np.float64)
def table(path):
    at, quiet = [], []
    with open(path) as f:
        f.readline()
        for line in f:
            c = line.split("\t"); at.append(int(c[1])); quiet.append(int(c[3]))
    return np.array(at), np.array(quiet)
def db(x):
    return 10 * math.log10(max(np.mean(x * x), 1e-9) / 32768**2)
for run in sys.argv[3:]:
    plays = [l.rstrip("\n").split("\t", 4) for l in open(os.path.join(data, run, "plays.tsv"))]
    outs = {m: wav(os.path.join(cache, f"828-d14-{run}-{m}", "segment-001-aec_only.wav")) for m in MODES}
    at, quiet = table(os.path.join(cache, f"828-d14-{run}-standard", "segment-001-hearing.tsv"))
    diff = {m: int(np.sum(outs[m] != outs["standard"])) for m in MODES[1:]}
    print(f"{run}: samples differing from standard: {diff} of {len(outs['standard'])}")
    for start, dur, voice, kind, text in plays:
        if kind == "story":
            continue
        s, d = int(start), int(dur)
        blocks = np.where((at >= s - 100_000) & (at <= s + d + 400_000))[0]
        if len(blocks) == 0:
            continue
        a, b = blocks[0] * 160, (blocks[-1] + 1) * 160
        print(f"  {text:22s} lamp_speaking={int(quiet[blocks[0]] < 30)} " + " ".join(
            f"{m}={db(outs[m][a:b]):6.1f}" for m in MODES))
