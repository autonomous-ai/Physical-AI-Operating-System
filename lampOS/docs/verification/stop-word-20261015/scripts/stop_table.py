"""Joins lamp-name check's output with the slice index: per setting, stop
words heard at 0.9 (over the lamp / with it quiet), other words, and false
hearings in the lamp's speech to nobody."""
import collections, os, sys
index_path, scores_path = sys.argv[1], sys.argv[2]
STOP = ("stop", "wait", "hold on")
index = {}
for line in open(index_path):
    run, mode, name, what, kind, speaking, text = line.rstrip("\n").split("\t")
    index[(run, mode, name)] = (what, speaking == "1", text)
highest, heard = {}, collections.defaultdict(list)
for line in open(scores_path):
    parts = line.rstrip("\n").split("\t")
    if len(parts) != 4:
        continue
    path, what, at, score = parts
    d, name = os.path.split(path)
    run_mode = os.path.basename(d)
    for mode in ("near-end", "transparent", "standard"):
        if run_mode.endswith("-" + mode):
            run = run_mode[: -len(mode) - 1]
            break
    key = (run, mode, name)
    if what == "highest":
        highest[key] = float(score)
    elif what == "heard":
        # A word's own hearing must begin after its 2 s lead-in.
        heard[key].append(float(at))
rows = collections.defaultdict(collections.Counter)
detail = []
for key, (what, speaking, text) in sorted(index.items()):
    run, mode, name = key
    hits = heard.get(key, [])
    if what == "word":
        group = "stop" if any(x in text.lower() for x in STOP) else "other"
        hit = any(t >= 1.8 for t in hits)
        cond = "over the lamp" if speaking else "lamp quiet"
        rows[mode][(group, cond, "n")] += 1
        rows[mode][(group, cond, "heard")] += hit
        detail.append((mode, run, text, cond, round(highest.get(key, 0), 3), hit))
    else:
        rows[mode][("nobody", "-", "n")] += 1
        rows[mode][("nobody", "-", "heard")] += bool(hits)
for mode in ("standard", "near-end", "transparent"):
    r = rows[mode]
    parts = []
    for group, cond in (("stop", "over the lamp"), ("stop", "lamp quiet"), ("other", "over the lamp"), ("other", "lamp quiet")):
        if r[(group, cond, "n")]:
            parts.append(f"{group} {cond}: {r[(group, cond, 'heard')]}/{r[(group, cond, 'n')]}")
    parts.append(f"false: {r[('nobody', '-', 'heard')]} of {r[('nobody', '-', 'n')]} 3 s pieces")
    print(f"{mode}: " + "; ".join(parts))
print()
for d in detail:
    if d[0] == "near-end" or d[0] == "standard":
        print("\t".join(str(x) for x in d))
