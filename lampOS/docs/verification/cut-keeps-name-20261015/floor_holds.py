"""Counts, in a voice's event records, how soon after the end of a turn the
coordinator held the floor (`floor_held`) and how soon the next turn was
admitted (`input_admitted`). Prints counts only.

    python3.12 -I floor_holds.py DIR   # DIR holds sessions/*/metadata/events-*.jsonl
"""

import collections
import glob
import json
import os
import sys

WITHIN_MS = 1000


def main(root):
    files = sorted(glob.glob(os.path.join(root, "**", "events-*.jsonl"), recursive=True))
    ends = 0
    holds = []  # (gap_ms or None, basis of the end)
    admitted = []  # gap_ms from an end to the next turn's admission
    sessions = set()
    for path in files:
        last_end = None  # (turn, at_us, basis)
        for line in open(path, errors="replace"):
            try:
                row = json.loads(line)
            except ValueError:
                continue
            kind = row.get("kind")
            if kind == "local_endpoint":
                ends += 1
                basis = (row.get("endpoint") or {}).get("basis", "silence_window")
                last_end = (row.get("turn"), row.get("at_us"), basis)
            elif kind == "floor_held":
                sessions.add(os.path.dirname(os.path.dirname(path)))
                if last_end and last_end[0] == row.get("turn"):
                    holds.append(((row["at_us"] - last_end[1]) / 1e3, last_end[2]))
                else:
                    holds.append((None, None))
            elif kind == "input_admitted" and last_end and last_end[0] is not None:
                if row.get("turn") == last_end[0] + 1:
                    admitted.append((row["at_us"] - last_end[1]) / 1e3)
    print(f"record files {len(files)}, ends {ends}, floor holds {len(holds)} in {len(sessions)} sessions")
    known = [h for h in holds if h[0] is not None]
    print(f"holds after an end of the turn they held: {len(known)} (others {len(holds) - len(known)})")
    soon = [h for h in known if h[0] <= WITHIN_MS]
    print(f"  of them within {WITHIN_MS} ms of that end: {len(soon)}, by the end's basis:",
          dict(collections.Counter(h[1] for h in soon)))
    print(f"next turns admitted after an end: {len(admitted)}; within {WITHIN_MS} ms: "
          f"{sum(1 for g in admitted if g <= WITHIN_MS)}")


if __name__ == "__main__":
    main(sys.argv[1])
