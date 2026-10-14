# What the model may do other than speak

Scope: `crates/voice/src/tools.rs`, the provider worker's `ToolDesk`
(`crates/voice/src/provider_worker.rs`), and function calls in the provider
link ([gemini-session-reliability.md](gemini-session-reliability.md),
"Function calls"). It is step 3 of [brain-build.md](brain-build.md), and the
written interface between the Voice and the Executive.

A Live session can be told of functions. The model asks for one by name; it
never supplies anything but that name and arguments that fit what was
declared. The voice declares three kinds and runs none itself.

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

**Not after acting.** A request in which the executive was asked to do
something is the lamp's: doing what was asked and then saying it was not
for the lamp cannot both be right, and what was done cannot be undone. A
call of `not_for_me` in such a request is answered and not taken, and
what the model says next is played. On lamp-4ace on 2026-10-14 the model
did exactly that three requests in a row, changing the light and saying
nothing, until its character said that changing its light is something
it does and says ([ring-choreography.md](ring-choreography.md), "The
lamp's own light").

Switched on with `LAMP_TOOLS=on` in the voice unit (the worker's `--tools`).
Off by default: what it does to those counts, and what a declared function
costs an ordinary answer's first audio, has not been measured.

## The executive's

The executive (`lamp-actions`, [actions.md](actions.md)) keeps two things in its
directory, which the voice is given as `LAMP_ACTIONS_DIR` (the worker's
`--actions-dir`, an absolute path):

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

### A word that asked for nothing, and a guard that was withdrawn

On lamp-4ace on 2026-10-14 a played "Thank you." was written by the
service as "cambiar", and the model called `light_colour` and said "I've
turned my light to yellow for you." Earlier that night a misheard word
had turned the volume up. Nobody had asked for either.

A guard was built for it the same morning and taken out within the hour.
It refused a call to the executive when the text of what was heard in
that request was one to three words and none of them was about a light
or a voice or said yes. On its first minutes on the lamp, "Brighter."
was played; the service wrote it as "Rainer"; the model, which had heard
it rightly, called `light_brighter`; and the guard refused the call. The
two requests to change the light that followed got no answer at all.

The text is the worse witness of the two. The model hears the sound; the
text is a second reading of it, and for a word or two it is often
another word ([attention.md](attention.md) found the same of the lamp's
name). A rule that lets the text overrule the model's hearing undoes
right actions as readily as wrong ones, and nothing else on the lamp
knows what a short word was. What is left is the character file's line:
change the light or the voice only when someone has plainly asked, in
words you made out. Whether that line is enough is not shown: the
mishearing cannot be made to happen on demand.

## The memory's

With `LAMP_MEMORY_SOCKET` in the voice unit (the worker's
`--memory-socket`, an absolute path) a session is also told of three
functions, declared by the voice and run by `lamp-memory`
([memory.md](memory.md), "How the voice uses it"):

| Function | When the model calls it | What it is told |
|---|---|---|
| `remember {text}` | Someone asks it to keep something | `{"remembered":true}`, and it says back what it kept |
| `recall {about?, days_ago?}` | Someone asks about an earlier conversation or day | `{"found":[{"when":"yesterday","heard":"...","you_said":"..."},{"when":"last week","asked_to_remember":"..."}]}`, or an empty list and "Nothing about that in what I remember." |
| `forget {}` | Someone asks it to forget the last thing it was asked to keep | `{"forgotten":true}`, or `false` with "There was nothing I had been asked to remember." |

They take the same road as the executive's: a thread of its own, 1.5 s at
most, at most eight waiting, the answer only to the connection that
asked. A memory that does not answer is told to the model as "My memory
is not answering right now, so I could not do that." An executive that
lists a function of one of these names does not get it.

## `the_time`, the voice's own

With `LAMP_CLOCK=on` in the voice unit (the worker's `--clock`, after
`--memory-socket`) a session is told of one more function, run in the
voice itself and answered at once: `the_time`, which the model is to call
whenever someone asks the time or the date or how long until something.
It is told `{"now":"It is 11:47 in the morning, Wednesday 14 October
2026. Said aloud: thirteen to twelve in the morning."}` from the lamp's clock and zone (`crates/voice/src/local_time.rs`),
or `{"error":"My clock could not be read just now."}`. Unlike
`not_for_me`, it is not silent: the model says what it was told.

Why: a session is told the time when it opens, and may run for hours. On
lamp-4ace on 2026-10-14 a session opened at 10:48 told someone at 11:17
that it was "roughly around ten forty-five": it was doing as told ("this
may be up to an hour old: if someone asks the time, give it roughly").
With the switch on, a new session is told instead that it woke at that
time, that it may be much later now, and to call `the_time` first.

What it costs: one call and its answer before the model speaks, for a
question about the time only. On lamp-4ace since 11:52 on 2026-10-14.

Played at 13:46 that day, three questions: "what time is it?" at 13:46 got
"It's just before two in the afternoon", "what day is it today?" got
"It's Wednesday, October fourteenth", and "what time is it now?" at 13:47
got "It's twenty to two": each asked the clock first (the voice's log
says so), and the first word came 1.0 to 1.1 s after the end of speech
was decided, as for other questions. The last is rounder than a person
would say at 13:47 ("quarter to two"); the function's description asks
for a person's way of saying it, and the model rounded to ten minutes.
At 12:08 someone at the lamp asked it, and it said "It's a little past
twelve.", which was right.

Played at 18:23 that day (`checks.sh steady2-1014`), "Hey Lamp, how are
you feeling right now?" made the model call the clock, and it answered
"It's twenty-three past in the evening.": not what was asked, and the hour
lost from "6:23 in the evening". So since `97880dc57` on `lamp-voice`
the clock's answer carries the time in the words a person says it in
("Said aloud: twenty-three past six in the evening": o'clock, past up to
half past, then to the next hour, quarter and half by name), and the
function's description says it is not for "now" or "today" in a question
about something else, such as how the lamp is feeling right now. On
lamp-4ace since 19:36 (build `e3a92c070`). Played at 19:51: "Hey Lamp, how
are you feeling right now?" got "I'm all warmed up and ready to chat! How
are things with you?" with no call to the clock; "Hey Lamp, what time is
it?" and "Lamp, what time is it now?" each called it, were told "Said
aloud: nine to eight in the evening.", and said "It's just about nine to
eight in the evening." and "It's right around nine to eight in the
evening." (the hedge is the model's: the minute was exact). One run each.
In the soak at 20:30 that evening "Good night." got "Night! It's
twenty-eight past eight in the evening. I'll keep the desk company while
you rest.": the time, unasked. So the description now ends "and never say
the time unless you were asked for it" (not yet played).

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

    lamp-voice provider-check /etc/lampOS/voice/provider.json --tools --written "NOTE"

opens one session told of `not_for_me` and of a sample `light_colour`,
sends the note as a written request, answers every call as done, and
prints which functions were called with what, when, what was said and how
the request ended. Nothing is captured, played or changed on the lamp.

## Not shown

Host tests: the router and the executive's client against a scripted
executive (six tests), the worker against the scripted provider with that
executive and with a memory on a socket (four), what a session is sent at
setup (one). Against the real service, silent written requests (the table
above, and one request to change the light's colour that called the
sample function with the colour asked for and then said "Got it! Blue it
is."), and on lamp-4ace on 2026-10-13 two spoken requests to the real
executive: "Hey Lamp, can you turn your volume down a bit?" called
`volume_down`, which answered in 84 ms, and the lamp said "Got it. How's
that?"; "Lamp, speak up a little, please." called `volume_up`. A request
that calls a function is answered about a second later than one that
does not: the model takes two turns. Not yet shown: that this model calls
`not_for_me` for overheard speech and not for speech meant for it. It
has been seen to call it for speech meant for the lamp: a new session's
first request, "Hey Lamp, how do I take my coffee?", before that session
had been told what the lamp was asked to remember. The memory's three
functions were each called once by the real service on the lamp
([memory.md](memory.md), "Played on the lamp").
