# The lamp speaks when spoken to: #829, step 2, 2026-10-16

Fourth's evidence for `LAMP_SPOKEN_TO=on`
([#829](https://github.com/autonomous-ai/lampOS/issues/829), [attention.md](../../attention.md#the-lamp-speaks-when-spoken-to-829-step-2)).
Read and run on Fourth's Intel Mac (macOS 26.6.2, i7-10700K, Rust 1.96.0).
Nothing was run on a lamp. The records are the first builder's, on the Mac
the two builders share (`~/lamp-voice-data/`). Counts only; words appear
only where they are the synthetic played lines, or one or two words with
nothing in them about anyone.

The two counting scripts are kept out of lampOS, which is Rust only. What
each does is below, so that it can be written again.

## What the switch would have held (`first-output`, 15 October)

lamp-4ace's first-output records of 15 October, 13:01 to 16:07
(`~/lamp-voice-data/sessions/turn-times-20261015/first-output`, 13 files):
69 decisions, 25 played and 44 held, all with `LAMP_ATTENTION=busy`.

The count: each played decision keeps playing with the switch if its
record shows a sign. The signs are the lamp's own turn, `going_on`,
`facing: true`, `named_sure`, `addressed: true`, a reason that is itself
a sign (the name, near, addressed, turned, the text's call, a conversation,
the lamp's own turn), or the word "lamp" in the heard text.

| Played | First sign |
|---|---|
| 12 | a conversation going on |
| 4 | the lamp's own turn |
| 4 | a face turned to the lamp |
| 1 | the name heard plainly |
| **4** | **none: held with the switch** |

The four:
- two one-word sounds ("igual", "O"), in a room not known to be busy, one
  of them answered with a call for a function;
- two were the rest of a call the lamp had cut at its comma ("... just in
  its reply without making any files.", #782), in a session the rule
  trusted. With `LAMP_CUT_KEEPS_NAME=on` (#853) the rest keeps the call's
  text, so it plays.

What the records cannot show: in a room not known to be busy the eyes were
not read, so `facing` is empty there. The four may have had a face turned
to the lamp, which would keep them, so 4 is the most it could have held.
The records keep only the sure name (0.999), not every hearing. A hearing
below that would also keep a request.

## What keeps a call with nobody facing the lamp (`scenarios`, 12 to 16 October)

The played scenarios on the shared Mac (`~/lamp-voice-data/scenarios`,
1,722 files at 02:41 on 16 October, the newest
`20261015T193803Z-day-night0233.txt`): each `PLAY` line that opens with the
name is a call, and what was heard is every `HEARD` line after it until
the next `PLAY`. The count asks whether that text shows the call to the
rule's `called` (the name or "lamb" anywhere, or a stand-in such as "man"
or "Milan" first or after a greeting). The count was ported line by line
from `crates/voice/src/attention.rs`.

| Call | Played | Nothing heard | Text shows the call | Text does not |
|---|---|---|---|---|
| "Lamp, ..." | 983 | 214 | **84** (11% of those heard) | 685 |
| "Hey Lamp, ..." | 1,593 | 178 | **242** (17%) | 1,173 |

Most often the service's text begins with the words after the name ("Tell
me a story about a lighthouse keeper, ..."). So the text is seldom what
keeps a call. The sound is: on 16 October at 02:05 the first builder ran
`lamp-name check` over the played lines of seven recordings (#828). With
the lamp quiet it heard "Hey Lamp, ..." 12 of 12 and "Lamp, stop." 0 of 2;
the model was trained on "Hey Lamp".

**So the switch's cost is "Lamp, ..." said by someone not facing the
lamp, outside a conversation with it: held about nine times in ten.**
Facing the lamp, or saying "Hey Lamp", it is answered. The first builder's
two-person scene uses "Hey Lamp, ..." for its named requests.

## Not shown

- Nothing was played. The pass mark is the first builder's (#829, 02:26):
  `twopeople829.sh` with the switch, the scene's first line left alone (0
  of 12), named requests 9 of 9, and the follow-up from the person in
  conversation still answered.
- How often a person at the lamp who faces it is seen as facing: the eyes'
  `facing` was never recorded in a quiet room.
- The second voice route takes the flag and does not apply it.
