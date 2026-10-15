"""Cut each replayed AEC-only output into one file per played word (2 s before
it to 0.8 s after) and 3 s pieces of the lamp speaking with nobody (1.5 s or
more from any played line), for `lamp-name check` with stop.onnx."""
import os, sys, wave
import numpy as np
cache, data, out = sys.argv[1], sys.argv[2], sys.argv[3]
MODES = ["standard", "near-end", "transparent"]
def table(path):
    at, quiet = [], []
    with open(path) as f:
        f.readline()
        for line in f:
            c = line.split("\t"); at.append(int(c[1])); quiet.append(int(c[3]))
    return np.array(at), np.array(quiet)
def write(path, samples):
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000)
        w.writeframes(samples.astype("<i2").tobytes())
index = open(os.path.join(out, "index.tsv"), "w")
for run in sys.argv[4:]:
    plays = [l.rstrip("\n").split("\t", 4) for l in open(os.path.join(data, run, "plays.tsv"))]
    for mode in MODES:
        d = os.path.join(cache, f"828-d14-{run}-{mode}")
        with wave.open(os.path.join(d, "segment-001-aec_only.wav")) as w:
            pcm = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
        at, quiet = table(os.path.join(d, "segment-001-hearing.tsv"))
        target = os.path.join(out, f"{run}-{mode}")
        os.makedirs(target, exist_ok=True)
        def block_at(us):
            return int(np.searchsorted(at, us))
        for n, (start, dur, voice, kind, text) in enumerate(plays):
            if kind == "story":
                continue
            s, e = int(start), int(start) + int(dur)
            a, b = block_at(s - 2_000_000), block_at(e + 800_000)
            speaking = int(quiet[min(block_at(s), len(quiet) - 1)] < 30)
            name = f"word-{n:02d}.wav"
            write(os.path.join(target, name), pcm[a * 160:b * 160])
            index.write(f"{run}\t{mode}\t{name}\tword\t{kind}\t{speaking}\t{text}\n")
        far = np.array([quiet[i] < 30 and all(not (int(p[0]) - 1_500_000 <= at[i] <= int(p[0]) + int(p[1]) + 1_500_000) for p in plays) for i in range(len(at))])
        piece, n = [], 0
        for i in range(len(at)):
            if far[i]:
                piece.append(i)
                if len(piece) == 300:
                    write(os.path.join(target, f"nobody-{n:02d}.wav"), pcm[piece[0] * 160:(piece[-1] + 1) * 160])
                    index.write(f"{run}\t{mode}\tnobody-{n:02d}.wav\tnobody\t-\t1\t-\n")
                    n += 1; piece = []
            else:
                piece = []
index.close()
