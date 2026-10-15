"""For each played word over the lamp, of the blocks in its window: how many
score as speech (>= 0.5), and of those how many the rule calls the lamp's own
voice (Own), a person (Over) or unjudgeable (Unknown); with the median share
of the raw microphone the speaker explains."""
import collections, os, sys
cache, data = sys.argv[1], sys.argv[2]
MODES = ["standard", "near-end", "transparent"]
STOP = ("stop", "wait", "hold on")
def rows(path):
    out = []
    with open(path) as f:
        h = f.readline().rstrip("\n").split("\t")
        for l in f:
            r = dict(zip(h, l.rstrip("\n").split("\t")))
            out.append((int(r["read_at_us"]), float(r["vad"]), int(r["render_quiet"]), r["heard"],
                        int(r["matched"]), int(r["capture_explained"]), int(r["residual_floor"])))
    return out
tot = {m: collections.Counter() for m in MODES}
for run in sys.argv[3:]:
    plays = [l.rstrip("\n").split("\t", 4) for l in open(os.path.join(data, run, "plays.tsv"))]
    for m in MODES:
        rs = rows(os.path.join(cache, f"828-d14-{run}-{m}", "segment-001-hearing.tsv"))
        for start, dur, voice, kind, text in plays:
            if kind == "story":
                continue
            s, d = int(start), int(dur)
            w = [r for r in rs if s - 100_000 <= r[0] <= s + d + 400_000]
            if not w or w[0][2] >= 30:
                continue
            group = "stop" if any(x in text.lower() for x in STOP) else "other"
            sp = [r for r in w if r[1] >= 0.5]
            c = collections.Counter(r[3] for r in sp)
            expl = sorted(r[5] for r in w if r[4])
            med = expl[len(expl)//2] if expl else None
            print(f"{run:6s} {m:11s} {text:20s} blocks={len(w):3d} speech={len(sp):3d} over={c['Over']:3d} own={c['Own']:3d} unknown={c['Unknown']:3d} explained_median={med}")
            t = tot[m]; t[group, "words"] += 1; t[group, "speech_blocks"] += len(sp)
            for k in ("Over", "Own", "Unknown"): t[group, k] += c[k]
print()
for m in MODES:
    t = tot[m]
    for g in ("stop", "other"):
        print(f"{m:11s} {g:5s}: {t[g,'words']} words, speech blocks {t[g,'speech_blocks']}: over {t[g,'Over']}, own {t[g,'Own']}, unknown {t[g,'Unknown']}")
