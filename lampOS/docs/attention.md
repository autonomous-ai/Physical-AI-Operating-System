# Whom the lamp answers

Scope: `crates/voice/src/attention.rs` and its use in the provider worker
(`crates/voice/src/provider_worker.rs`). Part of step 7 of
[brain-build.md](brain-build.md), brought forward on what the lamp did.

## The rule

The model decides what to say. For the first things a new session hears,
whether an answer is played is decided on the lamp:

| When | The lamp plays an answer |
|---|---|
| The request is one of a session's first two, and the room is busy (three things in a row it said nothing to, inside a minute; [character.md](character.md)) | only if it said something to someone in the last 45 seconds, or began the turn itself (a hello, a look), or the text of what it heard shows it was called |
| Any other time: a session that has heard more, a quiet room, a room not yet known | always: the model's own judgement stands, as before |

This is the second form of the rule. The first held back every answer in
a busy room unless the name was in the text, and failed on the lamp for
the reason given below. This form keeps the hold to the few requests
where the model is known to be unreliable, and reads the text only as
evidence for playing, never for holding.

## Why it is the lamp's to apply

On lamp-4ace on 2026-10-13 a played test cut the link to the provider
twelve times and each time let the new session hear two people talking to
each other. Three runs on the evening's builds:

| The session was | New sessions | Spoke words to the two people |
|---|---|---|
| told nothing about the room | 11 | 4 |
| told how the room was | 12 | 3 |
| told how the room was, and given a function to decline with ([tools.md](tools.md)) | 12 | 2 |

"How was your weekend?" was answered in all three. Nothing the session is
told or given has taken this away, and a difference of one or two in
twelve is not one that can be told from chance.

What makes a rule on the lamp possible is timing (what makes this one
unusable is further down). The service writes down
what it heard as well as what it says. In those three runs the text of
what was heard had arrived before the first sound of every one of the 24
answers, by 0.32 s at the least and 0.44 s in the middle. When an answer's
first sound comes the lamp can already read whether it was called, so
holding an answer back costs no time.

## What happens to an answer that is held back

It is dropped in the provider worker, as one the model declined itself is:
none of its sound reaches the coordinator or the speaker, and its words
are not kept as something the lamp said. The request ends when the service
completes it. The service still believes it answered; whether that
changes what it does next has not been looked at.

## Where it gives way

The rule fails open. The answer is played when no text of what was heard
has arrived, when the room is not known to be busy, and in any turn the
lamp began itself. The name is the word "Lamp", or "Lamb", which is how
the service writes it when it mishears; nothing else is taken for it.

## What it does not know yet

Whether the person is looking at the lamp. Someone who walks up to it in a
busy office and speaks without its name is held back, until the room has
been quiet for five minutes or they say its name once. The eyes' report of
who faces the lamp ([eyes.md](eyes.md), `facing`) is to be added.

## Played on the lamp, and why it is off

On lamp-4ace on 2026-10-13, with the rule on:

| Played | Result |
|---|---|
| Twelve new sessions, each hearing two people talk to each other | 0 of 11 judged sessions answered them. The rule held back the 4 answers the model began |
| Seven questions with its name, straight after, a few seconds apart | 7 of 7 answered |
| Room test: four questions without its name in a quiet room | 4 of 4 answered |
| Room test: six lines of two people talking | Nothing said |
| Room test, room busy: one question without its name | Left alone |
| Room test, room busy: two questions with its name ("Hey Lamp, ...", "Lamp, ...") | **0 of 2 answered. Both were held back** |

The last row is the rule failing at the thing it exists for. The service's
text of what it heard does not contain the name. For "Hey Lamp, what is
the capital of Italy?" it wrote "Milan, what is the capital of Italy?";
for "Lamp, what colour is the sky on a clear day?" it wrote "What color is
the sky on a clear day?". In 310 lines of what the lamp heard that day,
many of them played with the name, the word "Lamp" was written not once:
"Lam" and "Lan" three times each, and otherwise "Milan", "Man", "plan" or
nothing. The seven answered above were answered because each came within
45 seconds of the last, or before any text had arrived, not because the
name was read.

So the text of what was heard cannot say whether the lamp was called, and
the rule was switched off the same evening (21:26). What the runs do
show: the timing holds (the text arrives before the first sound), and
holding an answer back at the worker works. What is missing is a sign of
being called that does not come from that text. Candidates, none tried:
the model saying so itself through a silent function, the lamp spotting
its own name in the sound it captured, and the eyes' report that someone
is facing it.

## The second form

The rule as it now is (top of this page) differs in two ways. It applies
only to a session's first two requests, because that is where the answers
to other people's talk were: 9 of 35 new sessions within their first two
requests, against 2 of 87 things heard on a session that had been running
that afternoon. And the text is read only as evidence that the lamp was
called: its name, or one of the words the service has written in its
place at the start of a called request in the lamp's own records
("Milan,", "Man,", "Lam", "Lan", "plan"). A text with none of them proves
nothing, so outside those two requests nothing is held.

What it still costs: someone who calls the lamp in a busy room within its
first two requests after a new session, in words the text gives no sign
of, is not answered until they say it again. A session is renewed at most
every fifteen minutes in a busy room.

Since 2026-10-14 there is a second thing that can show the lamp was
called, and it does not depend on the text: the lamp's name heard in the
room's sound, by the part that listens for nothing else
([name.md](name.md)). The provider worker takes its hearings from
`name/heard`. A hearing counts for a request if the name began no more
than 1.5 s before the request did, or at any time while it is in hand:
the name is the first thing said, a little before the voice is sure
someone is speaking. Like the text, it can speak for playing an answer
and never against: a held request is played, and the log says
`its answer is played: the lamp's name was heard in the room's sound`.
With no such part running, or no bus, the rule goes by the text alone.

That narrows the cost above to the calls that part misses, which were
three in twenty on its first measurement, and adds one: a line that begins
with a name like the lamp's ("Hey Lan") now passes this rule in a busy
room on a new session, and is left to the model's own judgement as
everything was before the rule.

The cost it narrows had not been measured, and it was most of the calls.
The earlier count of new sessions called by name (14 of 16 below) was
made by starting the voice afresh, and a voice just started does not yet
know that its room is busy, so the rule never applied. On 2026-10-14 the
case itself was played (`namecall.sh`, nobody in the office): eleven
lines of two people talking to each other, so that the room is busy and
45 s have passed since the lamp spoke; the link to the service cut, which
gives a new session in the same process; then one question by name, in
one of three synthetic voices. Twelve rounds:

| | Rounds of 12 |
|---|---|
| The lamp said nothing through the two people's talk | 12 |
| The name was heard in the sound | 11 |
| The answer was let out because the name was heard, the text having shown nothing in time ("What is 2 + 2?") | 9 |
| The answer was let out by the text ("Milan, what comes after Tuesday?") | 2 |
| The answer was held back: the name was not heard and the text began "Helen," | 1 |
| The lamp was heard to answer | 9 |

So as the rule stood the day before, ten of these twelve answers would
have been held back from someone who had called the lamp by its name.
With the name heard in the sound it was one, and "Helen" is now among the
words the text may show a call by (with "Mom", "Amen" and "Weiland", each
written for the lamp's name at least once in its records).

Of the three rounds with no answer heard, the rule held one. In one the
service sent the words of an answer ("Four. Easy one.") and no sound, the
only time in the 800 and more answers in the lamp's records. In one it
wrote the question as "Milan, 1 vez que eu passo tu" and the model said
nothing. Where the lamp answered, its first sound came 0.98 to 1.24 s
after the end of speech was decided.

Not shown: a person, and a real office. The talk was two synthetic voices
from one loudspeaker, and the name part hears a synthetic "Hey Lamp"
better than it may hear a colleague's.

### A second look when the lamp heard its name and the model did not

The rule above only lets an answer out. It cannot make one. In the check
of two people talking ([character.md](character.md)), on 2026-10-14, the
closing "Hey Lamp, what do you think we should have for lunch?" got
nothing in two runs of two: the service wrote it as "Hey, what do you
think we should have for lunch?", it followed a conversation about lunch,
and the model took it for more of that. The part that listens for the
name had heard it both times, with a score of 1.000.

So the lamp now tells the model what its own ears heard
(`crates/voice/src/second_look.rs`, switched on with
`LAMP_SECOND_LOOK=on` in the voice unit, with the bus on). When a request
that was heard ends with nothing said, and the name began to be heard in
the sound no more than 1.5 s before that request began or at any time in
it, with a score of 0.98 or more, the voice begins one turn of its own
with a written note in place of speech, as it does for a hello:

> (Your own ears caught your name, Lamp, at the start of what was just
> said to you, so you were being called. If it is something a lamp on a
> desk could answer or chat about, answer it now in your usual way, as if
> you had not waited, and do not mention the wait or your name. If it is
> other people's own business, say nothing.)

What the model then says is spoken like any answer. The trace has
`second_look_due` with the score, and the turn's `input_admitted` has
`"admission_basis":"lamp_looks_again"`.

- **Once.** The turn the lamp begins is not a heard request, so whatever
  comes of it, nothing more is owed.
- **Someone who speaks first is listened to.** A request that begins
  before the second look has drops it, and one that could not begin
  within three seconds of the silence is dropped too.
- **It is late.** That nothing was said is only known when the service's
  wait for an answer has run out, four seconds after the person stopped
  (`no_reply_after_ms`). The answer comes a second or so after that.
- **A name like the lamp's.** Lines to "Lan" or "Liam" set the name part
  off in five of ten at the score it reports from (0.9) and in two of ten
  at 0.999 ([name.md](name.md)); 0.98 is between, and what such a line
  gets is then the note's last sentence and the model's judgement.

Played on lamp-4ace on 2026-10-14 from 06:04, nobody in the office:

| What was played | What should happen | What happened |
|---|---|---|
| Two people talking for nine lines, then the call and a follow-up without the name (`twopeople.sh`), four rounds | Nothing said through the talk; the call and the follow-up answered | Nothing said through the talk, and the call answered, in all four. In two the model answered by itself, its first words about 2 s after the caller stopped. In two it said nothing, the second look was taken, and its first words came about 5.3 s after the caller stopped. The follow-up was answered in all four, about 1.5 s after |
| Ten lines to someone with a name like the lamp's ("Hey Lan, did you see this?", "Hey Liam, the meeting moved.", `near.sh`), ten seconds apart | Nothing said | Nothing said to any of the ten. The name part reported four hearings (1.000, 0.903, 0.998, 1.000) and a second look was taken three times; told its name had been heard, the model still said nothing to other people's business |

Not shown: a person, a real office, and a line with a name like the
lamp's that a lamp could answer ("Hey Lan, what time is it?"), which the
note would let through.

### The same sign, given while the person is still speaking

A second look is late: five seconds after the caller stopped. The same
sign can be given in time. When the name is heard plainly (0.98 or more)
no more than 1.5 s before a request's sound began to be sent, or at any
time while it is being sent, the provider worker sends the service one
written note inside that request, beside the sound
(`crates/voice/src/called.rs`; `try_note` in `crates/gemini`, which the
service receives as `realtimeInput.text` within the request's activity):

> (For you to know, not to answer: your own ears caught your name, Lamp,
> just now. If what is being said is something a lamp on a desk could
> answer or chat about, it is said to you. Other people's own business
> still is not yours.)

It opens as the lines about the room do, and the character file says of
such a line that nobody said it aloud and that it is never to be answered
or mentioned. It is sent once for a request, and only while that
request's sound is still going to the service: sent at any other moment
it would end the link, so the link's own wrapper refuses it then. It is
switched on with `LAMP_NAME_NOTE=on` in the voice unit, which the voice
hands to the worker as `--name-note` (the worker is started with an empty
environment; the first build read the switch there and never sent a
note). The log has `the model is told the lamp's name was heard`.

Played on lamp-4ace on 2026-10-14 from 07:28, nobody in the office:

| What was played | What happened |
|---|---|
| Two people talking for nine lines, then the call and a follow-up, four rounds | Nothing said through the talk. The call answered in all four, its first words 2.2 to 2.5 s after the caller stopped. The model was told in three (the name heard at 1.000, 0.996 and 1.000); in the fourth the name part reported nothing and the model answered by itself. No second look was needed |
| Ten lines to someone with a name like the lamp's, ten seconds apart | Nothing said to any of the ten. The name part reported five hearings (0.977, 0.991, 0.966, 0.946, 1.000); the model was told twice and said nothing both times |

**What this shows is that the note arrives and does no harm.** It does
not show that the note is why the calls were answered: in the four rounds
played just before, with the note not reaching the service through the
build's fault, the model answered all four by itself, and in the four
before those it left two unanswered. Whether it raises the rate needs
many more rounds than twelve.

One thing to mend: when the model has been told inside a request and
still says nothing, the second look asks it again. That happened twice
with the lines to a similar name, and nothing was said either time, but
one asking is enough.

The rule has five tests of its own, and the note inside a request one
against a scripted service (it goes in order with the sound and the link
goes on). The worker's wiring has none; it was exercised on the lamp.

Played on lamp-4ace on the evening of 2026-10-13, with two synthetic
voices and nobody in the office:

| What was played | What should happen | What happened |
|---|---|---|
| The link cut twelve times, each new session then hearing two people talk to each other | Nothing said | 11 sessions could be judged; none answered within its first two things heard. The model kept quiet by itself in most; the rule held back two answers. Before the rule: 4 of 11, 3 of 12 and 2 of 12 |
| Seven questions put to the lamp by name, on a running session | Each answered | 6 answered, first sound 1.10 to 1.52 s after the end of speech was decided. The seventh, "Good night, Lamp.", never reached the model: the local gate judged it not speech, as it did in 5 of the 8 earlier runs of this test ([not-speech.md](not-speech.md)). Not the rule's doing |
| Quiet room, four questions without its name | Each answered | 4 of 4 |
| Two people talking to each other, six sentences, on a running session | Nothing said | Nothing said |
| Busy office, a question without its name ("What is the capital of Italy?") | Left alone | **Answered.** The session had been open a while, so the rule did not apply, and the model took a plain question as its own |
| Busy office, two questions by name | Each answered | Both answered; nothing was held back |

So the second form does what the first could not: it stops a new session
answering other people's talk without withholding an answer from someone
who called. It does not make a running session leave alone a clear
question that nobody put to it: that needs a sign the lamp does not have
yet, of whom a sentence was said to. And one of the called answers shows
what the missing name costs beyond this rule: "Hey Lamp, what is the
capital of Italy?" was heard as beginning "Milan", and the lamp answered
about Milan.

Later the same night, with the memory's three functions also declared
and the same rule, three more things were counted:

- **New sessions and other people's talk, twice more:** 2 of 12 sessions
  and then 1 of 12 answered within their first two things heard. Each
  time one stray answer started it: the lamp answered a third thing on a
  session (a scrap heard as one word; a remark about the weather), which
  the rule does not cover, and because it had then "said something to
  someone in the last 45 seconds" the rule let the next new session's
  first answer through. An answer nobody asked for excuses the next one.
- **New sessions and a question put to the lamp by name:** 14 of 16
  answered their first one, and both that did not had heard "Hey Lamp" as
  "Milan" and said nothing. With "Milan" and "Weiland" added to the names
  the character file says its ears make of "Lamp": 7 of 8, two of them
  heard as "Milan" and answered; the eighth was not heard at all.
- **Short phrases with the name at the end:** of five "Good night, Lamp."
  that reached the model, three were heard as "Good night, love" or
  "Good night, man" and left unanswered.

All three point the same way as the missing name in the text: the lamp
needs to know it was called from something other than the provider's
text. Measured on 346 examples from these records (`docs/policy.md`), a
sign from the device that the lamp was called takes "is this said to the
lamp" from about three in four right to about nineteen in twenty. That
sign, and the face turned to the lamp that the eyes now publish, are what
the part that decides whom the lamp answers is being built to use.

One more thing was tried and is not used. The model was asked to say when
it heard its name, through a function of the silent kind
(`LAMP_NAME_SIGNAL`). It did hear it: on 6 of 7 called questions on a
running session, and on none of the talk between two people. But having
made that sign it said nothing else: 1 of those 7 questions was answered.
A function of the ordinary kind would let it answer, about 0.7 s later.
Neither is acceptable, and the switch is off.

## Hearing its name in its own sound: first measurement

Not built into the lamp. A measurement of whether it can be.

A study on 2026-10-14, kept outside the repository, ran three kinds of
detector for "Hey Lamp" and "Lamp ơi" over 14,344 generated utterances
and found none that catches 85 calls in 100 at 2 false alarms in 100. It
recommended a small classifier of our own on an open speech front end
(openWakeWord's mel and embedding models), and as a fallback a ready-made
open keyword spotter that had caught the most "Hey Lamp" on generated
files. Both were written in Rust on the library the lamp already uses for
its sentence-end model.

The same night both were run on what the lamp's own microphone heard.
Six finite sessions of the voice with its recording switched on
(`lamp-voice directed ... --diagnostics`, the only way the voice writes
sound down), 86 s each, nobody in the office, 44 lines played from a
loudspeaker in eleven synthetic voices: twenty "Hey Lamp" calls, ten
lines to a similar name ("Hey Lan", "Hey Liam", "Hey Lance", "Hey Mom"),
ten of office talk, and four in Vietnamese. Each line was cut from the
recording by the time it was played and given to each detector whole.
Fired on, of the lines of each kind, on the sound before the echo
canceller:

| Detector | "Hey Lamp" (20) | A similar name (10) | Office talk (10) | "Lamp ơi" (2) |
|---|---|---|---|---|
| Our classifier, score 0.9 or more | 17 | 5 | 0 | 2 |
| Our classifier, 0.999 or more | 14 | 2 | 0 | 1 |
| The same with similar names in its training, 0.9 | 14 | 3 | 1 | 1 |
| The same, 0.999 | 10 | 2 | 0 | 1 |
| The ready-made keyword spotter | 2 | 1 | 0 | 0 |

After the echo canceller the counts differ by one or two and not in one
direction.

What it shows, and it is small:

- **The ready-made spotter does not survive the lamp's microphone.** On
  generated files it caught 70 to 82 of 100 calls; here 2 of 20. Louder
  copies of an earlier set of fourteen recordings changed nothing there
  either. It is not a fallback.
- **Our classifier, which has heard nothing but generated speech and
  never this room or this microphone, catches 14 to 17 of 20** and was
  set off by none of the office talk at either threshold. What sets it
  off is a name like its own.
- **Training it on similar names did not help here.** At 0.999 it caught
  10 calls where the first caught 14, and was set off by the same two
  names. At 0.9 it caught 14 against 17, was set off by three names
  against five, and by one line of office talk. That version was trained
  on more generated speech of the same kind; it says the next gain is
  not there.

So "called by name" can be one sign, right about seven times in ten and
quiet through ordinary talk, and cannot be a gate: three calls in ten are
missed, and a colleague called Lan or Lâm would set it off.

Not shown: a person. Every voice here was synthetic and played through a
loudspeaker near the lamp, loud enough that one recording clipped. 44 lines is too few to tell 14 from 17. The Vietnamese lines
are four. Nothing ran on the lamp's processor: the detectors ran on a
Mac over the recordings. What is needed next is real voices through the
lamp's microphone, which the same recording procedure gives, and the
microphone's sound on the bus so that a part of its own can listen.

## Switch

`LAMP_ATTENTION=on` in the voice unit (the worker's `--attention`). Off by
default. On on lamp-4ace since 22:15 on 2026-10-13.

## Not shown

Host tests: the rule and its bookkeeping (five, with the service's real
renderings of called requests as cases), and the worker with the scripted
provider in a room it is told is busy (one). The second form was played
once each way (the table above), with synthetic voices through a
loudspeaker and nobody in the room: not with people, and not on a day
when the office is full. The host tests give
the rule text with the name in it, which the real service does not
write: they did not catch the fault above, and the played room test
did.
