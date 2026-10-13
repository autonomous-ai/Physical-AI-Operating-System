# Keeping the thread across a new session

`crates/live/src/thread.rs`. Status on 2026-10-13: host tests, and two
renewals on lamp-4ace with the first wording of the note, which the lamp
spoke about (below). The present wording has not been through a renewal on a
lamp yet.

## Why

A session with the provider does not last. Gemini Live announces its own
close and a lost connection is replaced by a new one. On lamp-4ace the close
has been announced 3 minutes 20 seconds, 15 minutes and about 39 minutes
after a session opened, the two timed on 2026-10-13 with five minutes'
notice, so someone talking to the lamp for a quarter of an hour may well
meet one.
On lamp-4ace resumption is off (the relay does not forward it), so each new
session begins knowing nothing: a person in the middle of a conversation
finds the lamp has forgotten their name and what they were talking about.

## What it does

The provider worker keeps the last exchanges the lamp took part in, as the
provider's own transcripts gave them:

- what a person said, when the lamp began to answer within 20 seconds of it;
- what the lamp answered.

At most eight exchanges, 300 bytes of the person and 400 of the lamp in
each, with whitespace collapsed. Talk around the lamp that it did not answer
is not kept: it is dropped when the next thing is heard, or when an answer
begins more than 20 seconds later.

`Recalling`, which replaces the plain dialer in `run_with_continuation`, opens
every session through the same `Dialer` as before. When the thread holds an
exchange from the last ten minutes it first adds this to the end of the
instruction:

```text
Earlier in this conversation
This is what you and the person at your desk said to each other in the last
few minutes, for you to carry on from the way anyone carries on a
conversation they are in. It is background only: never read it out and never
answer any of it again.
Person: ...
You: ...
```

The note says nothing of sessions, connections or memory and does not tell
the lamp to keep quiet, and a test holds it to that. The first wording began
"Your connection was just renewed and you lost your memory of the
conversation you were in" and ended "Do not mention the renewal or this
note, do not repeat or answer any of it again, and if nobody speaks to you,
say nothing". On lamp-4ace on 2026-10-13, in a run of the conversation
checks with two renewals (one announced, one after a write to the provider
timed out), two things followed:

- The lamp opened the next answer after each renewal with "Sorry about
  that, my connection dropped!" and "My connection dropped for a second, but
  I'm all here!". Telling the model what not to mention told it what there
  was to mention.
- In the two runs of two people talking that came after, the lamp answered
  16 times with a spoken "...": it took its turn, showed that it was
  speaking, and said nothing. The question put to it by name and the one
  that follows it got "..." in both runs, where the same check had them
  answered an hour earlier. In the 527 scenarios recorded before those two
  there are three such replies. Without the note the model keeps quiet by
  not answering at all, which the service closes after four seconds as
  unanswered.

An answer with no letter or digit in it is also left out of what is told,
so a "..." cannot come back as something the lamp said.

With nothing said in the last ten minutes the thread adds nothing, so a
lamp left alone starts its next session as it always did. The instruction
may be 16,384 bytes in all (it was 8,192 until the character file grew past
seven thousand and left the thread room for three short exchanges). The
thread takes what the character file and the time of day
([character.md](character.md), "What the runtime adds to it") leave of
that, and when it does not all fit, the latest exchanges that do are told
and the older ones are left out. With the file lamp-4ace runs (about 7,100
bytes) all eight exchanges fit at their longest.

## What it keeps to itself

The thread lives in the provider worker's memory and nowhere else. It is not
written to disk and a restart of the service forgets it. The transcripts it
is built from are the ones the service already records in its session trace.

What a person said goes into the next session's instruction as they said it.
That is no more than they could say to the lamp directly.

## Verification

`thread::tests` (3 passed on macOS): that what was asked and answered is
kept, in pieces as it arrives, and the talk around it is not, nor an answer
with no word in it; that an exchange older than ten minutes is not told,
something said long before an answer is not taken for its question, only
eight exchanges of bounded length are held, and with less room the latest
that fit are told in order; that a new session's instruction is the written
one with the time and then the thread after it, with an empty thread and no
time is the written one unchanged, and with no room for the thread still
carries the time.

Not shown: that the model, told this in its present wording, does carry
on, does not bring the earlier conversation up by itself, and still stays
quiet for talk that is not for it. The character file is touchy (see [character.md](character.md)); a
renewal with a thread has to go through its checks before this is relied on.
