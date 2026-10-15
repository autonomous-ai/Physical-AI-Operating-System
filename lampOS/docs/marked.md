# Marking a bad moment for review

`crates/voice/src/marked.rs`. Issue
[#803](https://github.com/autonomous-ai/lampOS/issues/803). Played tests use
synthetic voices; real people talking to the lamp find what they miss. A
person who finds a moment wrong says so, and that moment is kept for the
first builder to read, with no form to fill in.

## What a person does and hears

The person says "Lamp, that was weird" (or "that was wrong", "bad answer",
"you interrupted me"). The model calls `mark_for_review` with their words
as `reason`, and says one short, warm thanks, "Thanks, I'll learn from
that.", with no apology and no question. The function's description tells
the model so, and its answer is `{"ok":true,"say":"Thanks, I'll learn from
that."}`.

Only words a person said can mark a moment. A request the lamp wrote itself
(a written turn) is refused: `{"error":"Only a person can mark a moment for
review."}`.

## What is kept

- One journal line, from the provider worker:
  `lamp-voice: marked for review: <reason>, session <id>, last 60 s`.
- A folder `/var/lib/lampOS/voice/marked/<unix ms>-<session id>/`, mode 0700,
  beside `sessions/`:
  - `events.jsonl`: the lines of the session's `metadata/events-*.jsonl`
    whose `at_us` lies in the 60 s before the call (the voice's monotonic
    clock, as the records' own), oldest first, as written. Turn records,
    transcripts and timing are all in those files. A line being written as
    it is read, or one that is not JSON, is left out.
  - `mark.json`: the reason (at most 200 bytes, control characters made
    spaces), the session id and folder, the time of the mark, the window's
    bounds in `at_us`, and how many lines were kept.
- No audio: the voice keeps none (sessions hold `metadata/` only).

The copy is outside `sessions/`, so `lamp-records`, which deletes whole
closed session folders (over 2 GB or 14 days, [records.md](records.md)),
never touches it. Nothing is held inside `sessions/`.

A mark is deleted 7 days after it was made: each new mark first deletes the
marks older than that, by the time in their folder's name. Nothing is
uploaded or published; marks stay on the lamp.

## Listing

`lamp-voice marked [MARKS]` lists the marks newest first, one line each:
local date and minute, reason, session, window, how many records, folder.
`MARKS` defaults to `/var/lib/lampOS/voice/marked`. The first builder reads
them each evening and files what they show as issues, with numbers only.

## The switch

`LAMP_MARK_WEIRD=on` on the voice service. The serving coordinator then
gives the provider worker (and the second route, which takes the same
flags) `--mark-weird <session folder>`, and the function is declared. The
worker needs the folder because it runs the call; the marks folder is
`marked/` beside the `sessions/` that holds it. Without the switch the flag
is not passed, the function is not declared, and a call to that name is
nobody's. A finite (`directed`) run never passes it.

## Verification

Host tests in `marked.rs` and `tools.rs`: the flag only with the switch on
and a session; the function declared and routed only with the flag, never
the executive's; a mark keeps exactly the 60 s window (lines at its edges
kept, one microsecond outside not, unfinished and non-JSON lines left out);
marks older than 7 days deleted and the rest listed newest first; a written
turn marks nothing.
