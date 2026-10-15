"""First-output decisions on lamp-4ace with a conversation going on: when,
the verdict, whether anyone was facing, and the length of what was heard.
Never the words: these are real people's."""
import datetime, glob, json, sys
for f in sorted(glob.glob(sys.argv[1] + "/*.jsonl")):
    for l in open(f):
        r = json.loads(l)
        if r.get("event") != "voice_first_output" or not r["baseline"]["going_on"]:
            continue
        b = r["baseline"]
        t = datetime.datetime.fromtimestamp(r["at_unix_ms"] / 1000)
        print(t.strftime("%m-%d %H:%M:%S"), r["permission"], "facing" if b["facing"] else "not-facing",
              r["reason"], f"session {r['session']} request {r['request']}",
              f"text_chars {len(b.get('text', ''))}", f"trigger {r['trigger']}", sep="\t")
