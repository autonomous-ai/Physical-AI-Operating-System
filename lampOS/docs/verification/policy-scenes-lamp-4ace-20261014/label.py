"""Join lamp-policy-judges choice records with the played scenes' own script.

Labels come only from what each played line was written to be (who it was
said to), never from what a judge or the lamp did. A choice whose heard text
matches no played line of its scene is left out and counted, so that nothing
said by anyone else in the room enters the file.

argv: JUDGES.jsonl OFFSET_US OUT.jsonl SPLITS SCENE.txt...
  OFFSET_US: the lamp's unix microseconds minus its monotonic microseconds.
  SPLITS: comma list of scene=split (train, calibration, test).
"""
import difflib
import json
import re
import sys

judges, offset_us, out_path, splits_arg, *scene_files = sys.argv[1:]
offset_us = int(offset_us)
splits = dict(item.split("=") for item in splits_arg.split(","))

TALK = {
    "So did you send the report to Maria yesterday?",
    "I did, but she has not replied yet. I think she is travelling this week.",
    "Right. Well, we need her numbers before Friday.",
    "I can call her tomorrow morning if you want.",
    "That would help. Also, the printer on the second floor is broken again.",
    "Again? That is the third time this month.",
    "I know. I already told the office manager.",
    "Okay. Are you coming to lunch with us?",
    "Maybe, I have a call at one.",
}
CALL = "Hey Lamp, what do you think we should have for lunch?"
FOLLOW = "And what would you drink with that?"
NEAR_NAME = re.compile(r"^Hey (Liam|Lan|man|Sam|Pam|Lance|Mom|love),")


def labels_for(text):
    """The seven questions and the addressee, as the line was written."""
    base = dict(calls_lamp=False, names_someone_else=False, others_business=False,
                lamp_could_answer=False, follows_lamp=False, listening_noise=False,
                asks_to_stop=False)
    if text in TALK:
        # Said by one colleague to the other; it names nobody as its addressee.
        return {**base, "names_someone_else": None, "others_business": True}, False
    if text == CALL:
        return {**base, "calls_lamp": True, "lamp_could_answer": True}, True
    if text == FOLLOW:
        # The caller's next question to the lamp, without its name.
        return {**base, "lamp_could_answer": True, "follows_lamp": True}, True
    if NEAR_NAME.match(text):
        return {**base, "names_someone_else": True, "others_business": True}, False
    raise SystemExit(f"no script label for a played line: {text!r}")


plays = []
for path in scene_files:
    session = re.sub(r"^\d{8}T\d{6}Z-|\.txt$", "", path.rsplit("/", 1)[-1])
    for line in open(path, encoding="utf-8"):
        m = re.match(r"#PLAY_US (\d+) (\w+): (.*)$", line.rstrip("\n"))
        if m:
            plays.append({"session": session, "at_us": int(m.group(1)), "text": m.group(3)})
plays.sort(key=lambda p: p["at_us"])
first, last = plays[0]["at_us"], plays[-1]["at_us"]


def similar(a, b):
    norm = lambda s: re.sub(r"[^a-z ]", "", s.lower())
    return difflib.SequenceMatcher(None, norm(a), norm(b)).ratio()


rows, left_out, seen = [], [], set()
for line in open(judges, encoding="utf-8"):
    record = json.loads(line)
    if record.get("event") != "choice":
        continue
    at_us = record["at_unix_ms"] * 1000 - offset_us
    if at_us < first or at_us > last + 25_000_000:
        left_out.append((record["number"], "outside the scenes"))
        continue
    heard = (record.get("context") or {}).get("heard") or ""
    # A line's choice comes after it ends; the next line may already have begun.
    candidates = [p for p in plays if 0 < at_us - p["at_us"] < 20_000_000]
    best = max(candidates, key=lambda p: similar(heard, p["text"]), default=None)
    score = similar(heard, best["text"]) if best else 0.0
    if best is None or score < 0.6:
        left_out.append((record["number"], f"heard text matches no played line (best {score:.2f})"))
        continue
    key = (best["session"], best["at_us"])
    if key in seen:
        left_out.append((record["number"], "a second choice for the same played line"))
        continue
    seen.add(key)
    labels, directed = labels_for(best["text"])
    rows.append({
        "schema": 1,
        "id": f"{best['session']}-decision-{record['number']}",
        "session": best["session"],
        "split": splits[best["session"]],
        "source": "lamp-4ace 2026-10-14 played scene (macOS voices, loudspeaker); lamp-policy-judges choice",
        "rights": "lampOS project recording, Apache-2.0",
        "cues": record["cues"],
        "labels": labels,
        "directed": directed,
        "baseline": record["baseline"],
    })

with open(out_path, "x", encoding="utf-8") as out:
    for row in rows:
        out.write(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n")
by = {}
for row in rows:
    k = (row["split"], row["directed"])
    by[k] = by.get(k, 0) + 1
print(f"rows {len(rows)}; by (split, directed): {dict(sorted(by.items()))}")
print(f"left out {len(left_out)}: " + "; ".join(f"#{n} {why}" for n, why in left_out))
