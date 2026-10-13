# Keeping the thread across a new session

`crates/live/src/thread.rs`. Status on 2026-10-13: host tests only. It has not
been through a renewal on a lamp yet.

## Why

A session with the provider does not last. Gemini Live announces its own
close after about 39 minutes, and a lost connection is replaced by a new one.
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
Your connection was just renewed and you lost your memory of the conversation
you were in. This is how it went until a moment ago, so that you can carry on
from it as if nothing had happened. Do not mention the renewal or this note,
do not repeat or answer any of it again, and if nobody speaks to you, say
nothing.
Person: ...
You: ...
```

With nothing said in the last ten minutes the instruction is sent exactly as
written, so a lamp left alone starts its next session as it always did. If
the instruction with the thread would be too long for the provider's limit,
the session opens without it.

## What it keeps to itself

The thread lives in the provider worker's memory and nowhere else. It is not
written to disk and a restart of the service forgets it. The transcripts it
is built from are the ones the service already records in its session trace.

What a person said goes into the next session's instruction as they said it.
That is no more than they could say to the lamp directly.

## Verification

`thread::tests` (3 passed on macOS): that what was asked and answered is
kept, in pieces as it arrives, and the talk around it is not; that an
exchange older than ten minutes is not told, something said long before an
answer is not taken for its question, and only eight exchanges of bounded
length are held; that a new session's instruction is the written one with
the thread after it, and with an empty thread is the written one unchanged.

Not shown: that the model, told this, does carry on, does not bring the
earlier conversation up by itself, and still stays quiet for talk that is not
for it. The character file is touchy (see [character.md](character.md)); a
renewal with a thread has to go through its checks before this is relied on.
