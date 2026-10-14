"""Step 7 of make.sh: the tables of docs/bare-name.md, from what evaluate.py wrote.

    python tables.py EVAL.json... [--point 0.9/1] [--stricter 0.99/1] [--label NAME]

Several files of one model are read as one.
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict

CONDITIONS = ["as recorded", "onset lost", "loudspeaker", "both", "talk around"]
WAYS = [("hey", '"Hey Lamp, ..."'), ("oi", '"Lamp ơi, ..."'), ("bare", '"Lamp, ..."')]


def pct(got: int, n: int) -> str:
    return f"{got}/{n} ({100 * got / n:.0f} %)" if n else ""


def calls_table(rows, point: str) -> list[str]:
    out = ["| Way of calling | Voices | " + " | ".join(CONDITIONS) + " |", "|---|---|" + "---:|" * len(CONDITIONS)]
    for way, label in WAYS:
        for who, keep in [
            ("generated, English reading", lambda r: r["set"] == "piper-calls-test" and r["accent"] == "en"),
            ("generated, Vietnamese reading", lambda r: r["set"] == "piper-calls-test" and r["accent"] == "vi"),
            ("generated, a British voice (Cori)", lambda r: r["set"] == "piper-calls-test" and r["accent"] == "cori"),
            ("Kokoro's test voices, English reading", lambda r: r["set"] == "kokoro-calls-test" and r["accent"] == "en"),
            ("Kokoro's test voices, Vietnamese reading", lambda r: r["set"] == "kokoro-calls-test" and r["accent"] == "vi"),
            ("real people (MSWC), the word alone", lambda r: r["set"] == "mswc-lamp-test"),
        ]:
            sel = [r for r in rows if r["way"] == way and keep(r)]
            if not sel:
                continue
            cells = [pct(sum(r["found"][f"{c}|{point}"] for r in sel), len(sel)) for c in CONDITIONS]
            out.append(f"| {label} | {who} | " + " | ".join(cells) + " |")
    return out


def near_table(near, points) -> list[str]:
    heads = [f"{how}, {p}" for p in points for how in ["as recorded", "both"]]
    out = ["| Near miss | Clips | " + " | ".join(heads) + " |", "|---|---:|" + "---:|" * len(heads)]
    for group, g in sorted(near.items()):
        cells = [pct(g["with_hearing"][f"{how}|{p}"], g["clips"]) for p in points for how in ["as recorded", "both"]]
        out.append(f"| {group} | {g['clips']} | " + " | ".join(cells) + " |")
    return out


def points_table(result, points) -> list[str]:
    rows = result["calls"]
    talk = result.get("talk", {})
    heads = ['"Hey Lamp"', '"Lamp ơi"', '"Lamp,"', '"Lamp," both', "Near misses, any"]
    heads += [f"{k}, an hour" for k in talk] + (["AMI meetings, an hour"] if "ami" in result else [])
    out = ["| Score from / steps in a row | " + " | ".join(heads) + " |", "|---|" + "---:|" * len(heads)]
    for p in points:
        cells = []
        for way, c in [("hey", "as recorded"), ("oi", "as recorded"), ("bare", "as recorded"), ("bare", "both")]:
            sel = [r for r in rows if r["way"] == way and r["set"] == "piper-calls-test"]
            cells.append(f"{100 * sum(r['found'][f'{c}|{p}'] for r in sel) / max(1, len(sel)):.0f} %")
        near = result.get("near", {})
        clips = sum(g["clips"] for k, g in near.items() if "own replies" not in k)
        hit = sum(g["with_hearing"][f"as recorded|{p}"] for k, g in near.items() if "own replies" not in k)
        cells.append(f"{100 * hit / max(1, clips):.1f} %")
        for k, t in talk.items():
            cells.append(f"{t['hearings'][p] / t['hours']:.2f} ({t['hearings'][p]} in {t['hours']:.1f} h)")
        if "ami" in result:
            a = result["ami"]
            cells.append(f"{a['hearings'][p] / a['hours']:.2f} ({a['hearings'][p]} in {a['hours']:.1f} h)")
        out.append(f"| {p} | " + " | ".join(cells) + " |")
    return out


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("eval", nargs="+")
    p.add_argument("--point", default="0.9/1")
    p.add_argument("--stricter", default="0.99/1")
    a = p.parse_args()
    result = {}
    for path in a.eval:
        part = json.load(open(path))
        # Talk measured in more than one run is put together.
        for key in [k for k in part if k.startswith("talk")]:
            result.setdefault("talk", {}).update(part.pop(key))
        result.update(part)
    if "calls" in result:
        print(f"## Calls at {a.point}\n")
        print("\n".join(calls_table(result["calls"], a.point)))
        print(f"\n## Calls at {a.stricter}\n")
        print("\n".join(calls_table(result["calls"], a.stricter)))
        points = sorted({k.split("|")[1] for k in result["calls"][0]["found"]}, key=lambda k: (int(k.split("/")[1]), float(k.split("/")[0])))
        print("\n## Operating points\n")
        print("\n".join(points_table(result, points)))
    if "near" in result:
        print(f"\n## Near misses at {a.point} and {a.stricter}\n")
        print("\n".join(near_table(result["near"], [a.point, a.stricter])))


if __name__ == "__main__":
    main()
