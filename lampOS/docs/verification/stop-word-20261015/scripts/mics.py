"""For each played word over the lamp: level in the word (dBFS) less the level
of the second before it (the lamp alone), on the second microphone before the
canceller and on the first microphone, raw. Higher means the word stands out."""
import json, os, sys, wave
import numpy as np
data = sys.argv[1]
def db(x):
    return 10 * np.log10(max(np.mean(x.astype(np.float64) ** 2), 1e-9) / 32768**2)
for run in sys.argv[2:]:
    d = os.path.join(data, run)
    src = [os.path.join(d, x) for x in os.listdir(d) if x.startswith(run + "-")][0]
    pre = np.fromfile(os.path.join(src, "capture-audio", "pre_aec.pcm16le"), dtype="<i2")
    first = None
    for line in open(os.path.join(src, "capture-audio", "events.jsonl")):
        r = json.loads(line)
        if r.get("kind") == "capture":
            first = r["meta"]["first_read_started_at_us"]; break
    with wave.open(os.path.join(d, "mic1.wav")) as w:
        rate1 = w.getframerate(); m1 = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
    start1 = int(open(os.path.join(d, "mic1.start_us")).read().strip())
    for line in open(os.path.join(d, "plays.tsv")):
        at, dur, voice, kind, text = line.rstrip("\n").split("\t", 4)
        if kind == "story":
            continue
        at, dur = int(at), int(dur)
        def cut(x, rate, t0, a, b):
            i, j = int((a - t0) * rate / 1e6), int((b - t0) * rate / 1e6)
            return x[max(i, 0):max(j, 0)]
        out = []
        for x, rate, t0 in ((pre, 16000, first), (m1, rate1, start1)):
            word = cut(x, rate, t0, at, at + dur)
            before = cut(x, rate, t0, at - 1_000_000, at - 50_000)
            out.append(f"{db(word):6.1f} vs {db(before):6.1f} ({db(word) - db(before):+5.1f})")
        print(f"{run:10s} {text:12s} mic2 pre-AEC {out[0]}   mic1 {out[1]}")
