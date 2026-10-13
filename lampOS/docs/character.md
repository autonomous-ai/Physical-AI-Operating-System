# The lamp's character

Who the lamp is, how it talks and when it keeps quiet are not in the
runtime. They are a text file the voice service is given as its system
instruction (`instruction_file` in the provider configuration, see
[live-runtime.md](live-runtime.md)). The one lamp-4ace runs is kept here
as [`deploy/character/lamp.txt`](../deploy/character/lamp.txt). Change it
there, install it on the lamp, and run the checks below again: several of
the lamp's behaviors exist only because of sentences in that file.

The runtime decides when the lamp may be heard, when it has been spoken
over and when a person has stopped. What the lamp then says, or whether it
says anything, is the model's, and the model does what this file tells it.

## Rules the file has to carry

Each was found on lamp-4ace with `gemini-3.8-live` and played prompts, by
changing the file and nothing else.

| Without the rule | The rule | Before | After |
|---|---|---|---|
| Spoken over in the middle of a story, the lamp stopped and then went on with the story instead of answering | "If someone starts talking while you are speaking, you have been interrupted… never finish the sentence you were in, never pick the story up again… answer only what they just said." | 0 of 3 answered the new request | 5 of 5 |
| Beside two people talking to each other it asked "Were you talking to me?" | What is for it and what is not, by what was said: something a lamp on a desk could answer or chat about is for it; other people's own business, a request for hands, an answer to someone else is not. "When you really cannot tell, stay silent. Never ask whether they were talking to you." | 2 of 9 lines answered with that question | 0 of 45 lines answered, in five runs |
| Told only to stay out of other people's talk, it also stayed out when called: "Hey Lamp, what should we have for lunch?" after such a conversation went unanswered in 7 of 10 runs. The service's own transcripts had the call as "Helen", "Weilan", "Wei Lan" and "Hey, man" | "Your ears often mishear it as some other name… So never go by the name you think you heard. Go by what was said." A sentence a lamp could answer is for it most of all when it starts with a call or a name, even one that is not its own, whatever was being said before | 3 of 10 answered | 5 of 5, and the follow-up without a name each time |
| Replies ended like an assistant's: "Anything you'd like to chat about today?", "Let me know if you need anything else." | Five short examples of how it sounds ("Thank you." "Anytime.") and three of how it does not | 3 of 7 replies to greetings and thanks ended that way | 0 of 18 |
| Speech over the lamp that came through garbled was transcribed as Spanish ("¿Qué?"), and the lamp, told always to speak English, answered "Sorry, I only speak English." | To that line: "and never remark on which language someone used. Speech you cannot make out is unclear speech, not another language." | 2 of 20 spoken-over scenes on 2026-10-13, one of them said twice | not measured yet |

The third row took three tries. A list of what its name can sound like
worked twice and then failed twice, and a firmer version of it failed
three times in three: the model heard "Wei Lan, what do you think…" after
two people had been talking and took Wei Lan for one of them. What worked was
to stop asking the model to recognise its name and ask it to judge the
sentence.

A change that looked harmless was not. Adding to the list of what the lamp
can do ("turn and lift your head a little, and glow") made it answer four
of four questions about lunch and dinner with "I can't recommend food, I'm
mostly a lamp", where it had been suggesting pasta. It was taken out again.
Any edit to the file can move a behavior it does not mention.

These counts are small, and each is played voices in one room. Model
behavior also varies from run to run on the same file: run every check
below after any change, more than once.

A fourth rule was tried and taken out. Since 2026-10-13 a short word
heard plainly over the lamp stops it and is sent to the model even when
nothing follows ([duplex-listening.md](duplex-listening.md), "A short word,
heard plainly"), so the file was made to say what to do with one: for
"stop" or "wait", stop; for "mm-hm", "yeah" or a noise, carry on from where
it was. Stop words did get "Okay." and silence ("Lamp, stop.", "Wait.",
"Okay, stop.", "Lamp, stop talking."). But a word spoken over the lamp
reaches the model through the canceller, and the service's transcripts of
such words were "hier" for "Yeah.", "mención" and "M&B Chan" for "Mm-hm.",
and "Mhm. Well. Well." for "Wait, wait, wait." With the carry-on rule in
the file that last one was followed by the rest of the story. A lamp that
goes on when it is told to wait is the worse mistake, so the file is back
to the one rule: whatever cuts in, the lamp lets go of what it was saying.
An acknowledgment heard plainly therefore ends the reply too, with an
"Okay." or a "Sorry, I missed that"; one heard faintly only makes the lamp
pause. Telling the two apart needs the word itself, and the lamp has that
only from the model.

The rule about staying quiet depends on the model being allowed to think:
with a thinking budget of 0 the lamp answered five of five lines not meant
for it, and with 128 it left alone nine of nine
([live-runtime.md](live-runtime.md), `thinking_budget`).

## Checks to run after changing the file

All four are played from another machine's speakers at the lamp; none needs
a person. What to play and what to expect:

| Check | Play | Expect |
|---|---|---|
| Addressed by name | Seven questions that begin "Lamp" or "Hey Lamp", with three lines for someone else among them ("Tom, did you send the invoice…", "I think we should move the meeting…", "Can you pass me the stapler, please?") | Seven answers; nothing for the three |
| Addressed without its name | Eight lines a person alone with the lamp might say ("Hello?", "Can you tell me a joke?", "I had a really long day today.", "Good night.") | Eight answers |
| Two people talking | Seven to nine lines of a conversation between two voices, less than a second apart, then "Hey Lamp, what do you think we should have for lunch?" and a follow-up without the name | Nothing during the conversation; both of the last two answered |
| Spoken over | A request for a long story, and four seconds into the answer a second voice: "Lamp, stop. What is two plus two?" | The lamp stops and the next thing it says is the answer, not the story |

On 2026-10-13 the file here gave: seven answers and nothing for the three;
8 of 8; nothing during the conversation and both questions answered, in
two runs; and for the last, of five requests spoken over the lamp that it
stopped for, four were answered. The fifth was "Wait, wait, wait.", which
reached the model as "wave wave wave", and the lamp went on with its story.

## What it does not decide

How fast the lamp answers, whether it hears someone over its own voice and
how long it waits for a person who has paused are the runtime's
([duplex-listening.md](duplex-listening.md),
[conversation-latency.md](conversation-latency.md)). A character file
cannot make the lamp stop for a voice its microphone did not pick out, and
it cannot make the service answer sooner.

## What the runtime adds to it

Two things are put after the file each time a session with the provider
opens. Neither is in the file, and neither is written anywhere.

**The time.** `crates/live/src/local_time.rs` reads the system's own
compiled zone file (`/etc/localtime`) and adds, for example:

```text
The time
It is Tuesday 13 October 2026, around 11:26 in the morning. You have no
clock, so this may be up to an hour old: if someone asks the time, give it
roughly.
```

The model has no clock. On lamp-4ace on 2026-10-13, at eleven in the
morning, it told two people who asked how it was that it was "enjoying the
quiet evening on the desk". A session lasts up to about three quarters of
an hour, which is why the note says how old it may be. Where the zone file
cannot be read, or the zone has summer time that the file leaves to a rule
past its listed changes, nothing is added. Host tests cover built zone
files of both layouts, days and years turning with the zone, and cut or
wrong files; the reader was also run once by hand on lamp-4ace's own file
and on this Mac's files for New York, London, Tokyo and Sydney, and agreed
with `date` for each. Not yet shown on the lamp: that the model uses it.

**The conversation so far**, when the previous session ended in the middle
of one: [conversation-thread.md](conversation-thread.md).
