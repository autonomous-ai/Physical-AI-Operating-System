"""Per played line of the two-people scenes: in how many runs the lamp spoke
(a LAMP line) before the next played line or the end. Counts only."""
import collections, glob, re, sys
runs = sorted(glob.glob(sys.argv[1]))
per_line = collections.OrderedDict()
by_family = collections.defaultdict(lambda: [0, 0, 0, 0])  # talk answered, talk lines, addressed answered, addressed lines
line_re = re.compile(r"^\s*([0-9.]+)s (PLAY|LAMP)\s+(.*)$")
for path in runs:
    events = []
    for line in open(path, errors="replace"):
        m = line_re.match(line)
        if m:
            events.append((float(m.group(1)), m.group(2), m.group(3)))
    family = re.sub(r"^[0-9TZ]+-", "", path.split("/")[-1]).rsplit("-two-people", 1)[0]
    plays = [i for i, e in enumerate(events) if e[1] == "PLAY"]
    for k, i in enumerate(plays):
        nxt = plays[k + 1] if k + 1 < len(plays) else len(events)
        answered = any(e[1] == "LAMP" for e in events[i + 1:nxt])
        text = events[i][2]
        stat = per_line.setdefault(text, [0, 0])
        stat[0] += answered; stat[1] += 1
        addressed = "Lamp" in text or text.startswith("Daniel: And what would")
        f = by_family[family]
        if addressed:
            f[2] += answered; f[3] += 1
        else:
            f[0] += answered; f[1] += 1
for text, (a, n) in per_line.items():
    print(f"{a:3d}/{n:3d}  {text}")
print()
for fam, (ta, tn, aa, an) in sorted(by_family.items()):
    print(f"{fam:22s} talk answered {ta}/{tn}  addressed answered {aa}/{an}")
