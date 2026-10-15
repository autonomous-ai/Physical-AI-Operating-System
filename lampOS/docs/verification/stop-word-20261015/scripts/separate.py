"""Speech-like blocks (score >= 0.5) while the lamp speaks: in the stop words'
windows against the lamp's speech to nobody, by how much of the raw
microphone the speaker explains. Would any explained_percent bar separate them?"""
import os, sys
import numpy as np
cache, data = sys.argv[1], sys.argv[2]
STOP = ("stop", "wait", "hold on")
for mode in ("standard", "near-end"):
    word, nobody = [], []
    for run in sys.argv[3:]:
        plays = [l.rstrip("\n").split("\t", 4) for l in open(os.path.join(data, run, "plays.tsv"))]
        with open(os.path.join(cache, f"828-d14-{run}-{mode}", "segment-001-hearing.tsv")) as f:
            f.readline()
            for line in f:
                c = line.rstrip("\n").split("\t")
                at, vad, quiet, matched, expl, floor, age = int(c[1]), float(c[2]), int(c[3]), int(c[4]), int(c[5]), int(c[6]), int(c[7])
                if quiet >= 30 or vad < 0.5 or not matched or age > 40:
                    continue
                near = [p for p in plays if p[3] != "story" and int(p[0]) - 100_000 <= at <= int(p[0]) + int(p[1]) + 400_000]
                far = all(not (int(p[0]) - 1_500_000 <= at <= int(p[0]) + int(p[1]) + 1_500_000) for p in plays)
                if near and any(x in near[0][4].lower() for x in STOP):
                    word.append((expl, floor))
                elif far:
                    nobody.append((expl, floor))
    w, n = np.array(word), np.array(nobody)
    print(f"{mode}: speech-like blocks in stop-word windows {len(w)}, in the lamp's speech to nobody {len(n)}")
    for bar in (75, 80, 85, 90, 93, 95, 97):
        print(f"  explained <= {bar}: words {np.mean(w[:,0] <= bar)*100:5.1f}%  nobody {np.mean(n[:,0] <= bar)*100:5.1f}% ({int(np.sum(n[:,0] <= bar))} blocks)")
    print(f"  explained, words: median {np.median(w[:,0]):.0f} [p25 {np.percentile(w[:,0],25):.0f}, p75 {np.percentile(w[:,0],75):.0f}]; nobody: median {np.median(n[:,0]):.0f} [p25 {np.percentile(n[:,0],25):.0f}, p75 {np.percentile(n[:,0],75):.0f}]")
