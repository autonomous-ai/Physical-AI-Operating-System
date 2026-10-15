"""First-output decisions on lamp-4ace let out because a conversation was
going on: which played line each answered (by the lamp's clock), and
whether anyone was facing. Synthetic played lines only."""
import bisect, glob, json, sys
fo, scenes = sys.argv[1], sys.argv[2]
plays = []
for path in glob.glob(scenes + "/*.txt"):
    name = path.rsplit("/", 1)[-1]
    for line in open(path, errors="replace"):
        if line.startswith("#PLAY_US "):
            _, us, text = line.rstrip("\n").split(" ", 2)
            plays.append((int(us), text, name))
plays.sort()
keys = [p[0] for p in plays]
for f in sorted(glob.glob(fo + "/*.jsonl")):
    for l in open(f):
        r = json.loads(l)
        if r.get("event") != "voice_first_output" or not r["baseline"]["going_on"]:
            continue
        began = r["input_began_us"]
        i = bisect.bisect_right(keys, began + 1_000_000) - 1
        hit = plays[i] if i >= 0 and began - plays[i][0] < 4_000_000 else None
        prev = plays[i - 1] if hit and i > 0 and plays[i - 1][2] == hit[2] else None
        print(f"{r['permission']:4s} facing={r['baseline']['facing']!s:5s} {r['reason'][:44]:44s} | line: {hit[1] if hit else '?'} | before it: {prev[1] if prev else '-'} | {hit[2] if hit else ''}")
