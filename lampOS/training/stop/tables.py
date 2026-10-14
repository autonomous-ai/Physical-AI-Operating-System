"""Step 7 of make.sh: the tables of docs/stop-word.md, from what evaluate.py wrote.

    python tables.py EVAL.json [--point 0.9/1] [--stricter 0.98/1]
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict

import numpy as np

LEVELS = ["clean", 0, -5, -10, -15, "suppressed"]
TARGETS = ["stop", "wait", "hold on"]


def head(level) -> str:
    return {"clean": "alone", "suppressed": "-10, suppressed"}.get(level, f"{level} dB")


def catch_table(rows, point: str, other: str) -> list[str]:
    cells = defaultdict(lambda: [0, 0])
    for r in rows:
        if r["other"] != other and r["level"] != "clean":
            continue
        real = "real people" if r["source"] != "Piper" else "generated voices"
        for key in [(r["target"], r["source"]), (r["target"], real)]:
            c = cells[key + (r["level"],)]
            c[0] += r["found"][point] is not None
            c[1] += 1
    out = ["| Word | Speakers | " + " | ".join(head(lv) for lv in LEVELS) + " |", "|---|---|" + "---:|" * len(LEVELS)]
    for target in TARGETS:
        sources = sorted({k[1] for k in cells if k[0] == target and k[1] not in ("real people", "generated voices")})
        for source in sources + ["real people", "generated voices"]:
            row = []
            for lv in LEVELS:
                got, n = cells.get((target, source, lv), [0, 0])
                row.append(f"{got}/{n} ({100 * got / n:.0f} %)" if n else "")
            if any(row):
                label = f"**{source}**" if source in ("real people", "generated voices") else source
                out.append(f"| {target} | {label} | " + " | ".join(row) + " |")
    return out


def per_hour(block, key: str) -> str:
    h = block["hours"]
    n = block["hearings"].get(key, 0)
    return f"{n / h:.2f} ({n} in {h:.1f} h)"


def operating_points(result, points) -> list[str]:
    rows = result["catches"]
    out = [
        "| Score from / steps in a row | Real people caught, level | Real people, -10 dB | Real people, -15 dB | Other voice alone, readers, an hour | Generated voice alone, an hour | AMI meetings, an hour | Near misses with a hearing |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for point in points:
        rec = {}
        for level in [0, -10, -15]:
            sel = [r for r in rows if r["source"] != "Piper" and r["level"] == level and r["other"] == "reader"]
            rec[level] = 100 * np.mean([r["found"][point] is not None for r in sel]) if sel else float("nan")
        near = result.get("near", {})
        clips = sum(g["clips"] for g in near.values())
        hit = sum(g["with_hearing"].get(point, 0) for g in near.values())
        out.append(
            f"| {point} | {rec[0]:.0f} % | {rec[-10]:.0f} % | {rec[-15]:.0f} % | {per_hour(result['talk_reader'], point)} | "
            f"{per_hour(result['talk_generated'], point)} | {per_hour(result['ami'], point)} | {hit}/{clips} ({100 * hit / max(clips, 1):.1f} %) |"
        )
    return out


def near_table(result, points) -> list[str]:
    out = ["| Near miss | Clips | " + " | ".join(f"With a hearing at {p}" for p in points) + " |", "|---|---:|" + "---:|" * len(points)]
    for group, g in sorted(result["near"].items()):
        cells = [f"{g['with_hearing'].get(p, 0)} ({100 * g['with_hearing'].get(p, 0) / g['clips']:.0f} %)" for p in points]
        out.append(f"| {group} | {g['clips']} | " + " | ".join(cells) + " |")
    return out


def latency(rows, point: str) -> list[str]:
    out = ["| Level | Caught | From the word's end to the hearing: median | 90th percentile |", "|---|---:|---:|---:|"]
    for level in LEVELS:
        d = [r["found"][point] for r in rows if r["level"] == level and r["found"][point] is not None and (r["other"] == "reader" or level == "clean")]
        if d:
            out.append(f"| {head(level)} | {len(d)} | {1000 * np.median(d):.0f} ms | {1000 * np.percentile(d, 90):.0f} ms |")
    return out


def phrase_table(rows, point: str) -> list[str]:
    """Generated voices, by phrase, at 0 dB under a reader: which phrases are hard."""
    by = defaultdict(lambda: [0, 0])
    for r in rows:
        if r["source"] == "Piper" and r["level"] == -10 and r["other"] == "reader":
            c = by[r["text"]]
            c[0] += r["found"][point] is not None
            c[1] += 1
    out = ["| Phrase (generated, -10 dB under a reader) | Caught |", "|---|---:|"]
    for text, (got, n) in sorted(by.items(), key=lambda kv: kv[1][0] / kv[1][1]):
        out.append(f"| {text} | {got}/{n} |")
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("eval")
    p.add_argument("--point", default="0.9/1")
    p.add_argument("--stricter", default="0.98/1")
    a = p.parse_args()
    result = json.load(open(a.eval))
    rows = result["catches"]
    print(f"## Catches at {a.point}, under a LibriSpeech test reader\n")
    print("\n".join(catch_table(rows, a.point, "reader")))
    print(f"\n## Catches at {a.point}, under a held-out generated voice\n")
    print("\n".join(catch_table(rows, a.point, "generated")))
    print(f"\n## Catches at {a.stricter}, under a LibriSpeech test reader\n")
    print("\n".join(catch_table(rows, a.stricter, "reader")))
    points = sorted({k for r in rows[:1] for k in r["found"]}, key=lambda k: (int(k.split("/")[1]), float(k.split("/")[0])))
    print("\n## Operating points\n")
    print("\n".join(operating_points(result, points)))
    print(f"\n## Near misses at {a.point} and {a.stricter}\n")
    print("\n".join(near_table(result, [a.point, a.stricter])))
    print(f"\n## How soon, at {a.point}\n")
    print("\n".join(latency(rows, a.point)))
    print(f"\n## Phrases at {a.point}\n")
    print("\n".join(phrase_table(rows, a.point)))
    ami = result["ami"]
    print(f"\n## AMI: hearings within 1.5 s after a transcribed target ({ami['targets_said']} said)\n")
    print(json.dumps(ami["near_said_target"]))
    print("\nAt 0.9/1, what was said before each hearing:\n")
    for d in ami["details_at_0.9"]:
        print(f"- {d['meeting']} {d['t']} s, {d['score']:.3f}{' (a target was said)' if d['target_said'] else ''}: \"{d['words']}\"")


if __name__ == "__main__":
    main()
