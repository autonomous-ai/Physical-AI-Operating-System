# What the model may do other than speak

Scope: `crates/live/src/tools.rs`, the provider worker's `ToolDesk`
(`crates/live/src/provider_worker.rs`), and function calls in the provider
link ([gemini-session-reliability.md](gemini-session-reliability.md),
"Function calls"). It is step 3 of [brain-build.md](brain-build.md), and the
written interface between the Voice and the Executive.

A Live session can be told of functions. The model asks for one by name; it
never supplies anything but that name and arguments that fit what was
declared. The voice declares two kinds and runs neither itself.

## `not_for_me`, the voice's own

A session has had one thing to do with what it hears: answer it. On
lamp-4ace on 2026-10-13, 23 new sessions each heard two people talking to
each other: 7 spoke words to them and 3 more made a sound, and telling the
session how the room was changed nothing (4 of 11 without, 3 of 12 with).
`not_for_me` gives the model something else to do with speech that is not
for it. It takes no arguments. When the model calls it for a request:

- the call is answered at once (`{"ok":true}`);
- whatever sound the service still sends for that request is dropped in the
  provider worker, before the coordinator and the speaker, and its words are
  not kept as something the lamp said;
- the request ends when the service completes the turn it asked in.

It is declared as a function nothing is to be said on account of
(`Tool::silent`: the service need not wait for it, and its answer is taken
note of without generating). That was chosen on what the real service did
with written requests on lamp-4ace on 2026-10-13, each on a session of its
own, with sentences said to someone else:

| Declared as | Requests | Called it | What followed a call |
|---|---|---|---|
| An ordinary function | 5 | 3 | Once "I'm sorry, I missed that. Could you please repeat it?", once a few words of nonsense in French, once the same call eleven times in a row, each answer bringing the next |
| A silent one | 7 | 2 | Nothing said, and the request over within 4 ms of the call (0.63 s and 0.85 s after the request) |

The requests in which it was not called were not answered at all, and were
closed by the four-second wait as before. None of the twelve was answered
as if it had been said to the lamp. These were written requests; what the
model does with the same sentences heard as speech is not known from them.

Switched on with `LAMP_TOOLS=on` in the voice unit (the worker's `--tools`).
Off by default: what it does to those counts, and what a declared function
costs an ordinary answer's first audio, has not been measured.

## The executive's

The executive (`lamp-exec`, not yet built) keeps two things in its
directory, which the voice is given as `LAMP_EXEC_DIR` (the worker's
`--exec-dir`, an absolute path):

    <dir>/tools.json    {"tools":[{"name":"light_on","description":"...","parameters":{...}}]}
    <dir>/call.sock     a Unix stream socket

`tools.json` is read each time a session opens, so the executive's
functions appear when it starts and go when its directory does. It is all
or nothing: a file over 64 KiB, one that is not that shape, a function the
provider link would refuse (a name that is not `[a-z][a-z0-9_]{0,47}`, a
description over 1,024 bytes, parameters that are not an object or are over
4,096 bytes) or a name twice leaves the session with none of them. At most
32 functions reach a session, the voice's own included; `not_for_me` in the
file is ignored.

A call is one line to the socket and one line back, on a connection of its
own:

    {"op":"call","name":"light_colour","args":{"colour":"blue"}}
    {"ok":true,"result":{"light":"blue"}}
    {"ok":false,"error":"The light is already at its brightest."}

`result` goes to the model as it is; `error` goes as `{"error": "..."}`,
and should be words the lamp can say. The answer may be at most 8,192
bytes on the line and about 4,000 as JSON to the model; a larger one is
treated as no answer.

## What the voice promises, and what it does not

- **Every call is answered.** The service generates nothing more for a
  request until its calls are, so a call nobody runs is answered too.
- **The voice waits 1.5 s for the executive and no longer**
  (`ASK_DEADLINE`), connecting included, on a thread of its own: the
  worker's loop, the microphone and the speaker are never held. With the
  executive gone, late, cut off or answering something that is not an
  answer, the model is told "That part of me is not running right now, so
  I could not do it." and the conversation goes on.
- **At most eight calls wait at once.** A ninth is answered as unavailable.
- **An answer goes only to the connection that made the call.** If the
  link dropped meanwhile, the answer is not sent and the call is over.
- **Nothing is retried,** and the voice does not know whether the
  executive did what it was asked when no answer came. The executive owns
  that: a command that was tried is finished, and what it reports is what
  it established.
- **Names are logged, arguments never.**

## Trying it without a sound

    lamp-live provider-check /etc/lampOS/voice/provider.json --tools --written "NOTE"

opens one session told of `not_for_me` and of a sample `light_colour`,
sends the note as a written request, answers every call as done, and
prints which functions were called with what, when, what was said and how
the request ended. Nothing is captured, played or changed on the lamp.

## Not shown

Host tests: the router and the executive's client against a scripted
executive (five tests), the worker against the scripted provider and that
executive (three), what a session is sent at setup (one). Against the real
service, silent written requests only (the table above, and one request to
change the light's colour that called the sample function with the colour
asked for and then said "Got it! Blue it is."). Not yet shown: that this
model calls `not_for_me` for overheard speech and not for speech meant for
it, and the cost in time of declared functions on an ordinary answer.
There is no executive yet.
