"""#828: how each played word is heard under each suppressor setting.

python3.12 -I hearing.py CACHE_DIR DATA_DIR RUN [RUN ...]

CACHE_DIR holds the replays (828-d14-<run>-<mode>/segment-001-hearing.tsv,
written by hearing_replay); DATA_DIR the 14 Oct runs (<run>/plays.tsv and
<run>/<run>-*/events.jsonl). Counts only; the played words are the task's
own synthetic lines.
"""

import collections
import glob
import json
import os
import sys

MODES = ["standard", "near-end", "transparent"]
ONSET = 7  # Confirmation::EVALUATED.onset_blocks
PLAIN = 10  # Confirmation::EVALUATED.plain_blocks
AFTER_US = 400_000  # a word's window runs this long past its end
CLEAR_OF_WORDS_US = 1_500_000  # lamp speech this far from any play is "nobody"
SPEAKING_QUIET = 30  # Hearing::EVALUATED.clear_after_blocks


def table(path):
    rows = []
    with open(path) as f:
        header = f.readline().rstrip("\n").split("\t")
        for line in f:
            r = dict(zip(header, line.rstrip("\n").split("\t")))
            rows.append({
                "at": int(r["read_at_us"]),
                "vad": float(r["vad"]),
                "quiet": int(r["render_quiet"]),
                "heard": r["heard"],
                "plain": r["plain"] == "1",
            })
    return rows


def plays(path):
    out = []
    for line in open(path):
        at, dur, voice, kind, text = line.rstrip("\n").split("\t", 4)
        out.append((int(at), int(dur), voice, kind, text))
    return out


def longest_over(rows):
    best = run = 0
    for r in rows:
        run = run + 1 if r["heard"] == "Over" else 0
        best = max(best, run)
    return best


def lamp_outcomes(events_path):
    """What the lamp itself did: duplex records with their times."""
    out = []
    for line in open(events_path):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if r.get("kind") in ("duplex_yield", "duplex_commit", "duplex_yield_end"):
            out.append((r.get("at_us"), r["kind"], r.get("outcome") or r.get("reason")))
    return out


def main(cache, data, runs):
    per_mode = {m: collections.Counter() for m in MODES}
    false_yields = {m: 0 for m in MODES}
    lamp_minutes = {m: 0.0 for m in MODES}
    words = []
    for run in runs:
        played = plays(os.path.join(data, run, "plays.tsv"))
        events = glob.glob(os.path.join(data, run, f"{run}-*", "events.jsonl"))
        outcomes = lamp_outcomes(events[0]) if events else []
        for mode in MODES:
            path = os.path.join(cache, f"828-d14-{run}-{mode}", "segment-001-hearing.tsv")
            if not os.path.exists(path):
                continue
            rows = table(path)
            for at, dur, voice, kind, text in played:
                if kind == "story":
                    continue
                window = [r for r in rows if at - 100_000 <= r["at"] <= at + dur + AFTER_US]
                speaking = any(r["quiet"] < SPEAKING_QUIET for r in window[:5])
                over = longest_over(window)
                plain = sum(r["plain"] for r in window)
                yields = over >= ONSET
                commits = yields and plain >= PLAIN
                did = [o for o in outcomes if o[0] and at - 100_000 <= o[0] <= at + dur + 2_000_000]
                words.append((run, mode, kind, text, speaking, over, plain, yields, commits,
                              ",".join(k.replace("duplex_", "") for _, k, _ in did)))
                if speaking:
                    c = per_mode[mode]
                    c[(kind, "words")] += 1
                    c[(kind, "yield")] += yields
                    c[(kind, "commit_plain")] += commits
            # Nobody: lamp speaking, far from every played line.
            far = [r for r in rows if r["quiet"] < SPEAKING_QUIET and all(
                not (a - CLEAR_OF_WORDS_US <= r["at"] <= a + d + CLEAR_OF_WORDS_US)
                for a, d, *_ in played)]
            lamp_minutes[mode] += len(far) / 6000
            run_len = 0
            for r in far:
                run_len = run_len + 1 if r["heard"] == "Over" else 0
                if run_len == ONSET:
                    false_yields[mode] += 1
    print("run\tmode\tkind\ttext\tlamp_speaking\tlongest_over\tplain\tyields\tcommits_plain\tlamp_did")
    for w in words:
        print("\t".join(str(x) for x in w))
    print()
    for mode in MODES:
        c = per_mode[mode]
        kinds = sorted({k for k, _ in c})
        summary = "; ".join(
            f"{k}: {c[(k, 'yield')]}/{c[(k, 'words')]} yield, {c[(k, 'commit_plain')]} plain stop"
            for k in kinds)
        print(f"{mode}: {summary}; false yields {false_yields[mode]} "
              f"in {lamp_minutes[mode]:.1f} min of the lamp speaking with nobody")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
