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
| Beside two people talking to each other it asked "Were you talking to me?" | What is for it (its name, what follows from its own conversation, what a person says to a lamp on their desk), what is not (other people's plans and work, an answer to someone else, a request for hands), and "When you cannot tell, stay silent. Never ask whether they were talking to you." | 2 of 9 lines answered with that question | 0 of 39 lines answered, in five runs |
| With that rule in place, "Hey Lamp, what should we have for lunch?" after such a conversation went unanswered: the service's own transcript had it as "Helen, what do you think…", and another time as "Weilan" | "Your name is Lamp… Your ears often get it wrong: it can come out as 'lamb', 'Helen', 'Weilan', 'Liam'… Nobody in the room is called any of those. When a sentence begins with a word that sounds anything like 'Lamp'… it is you they are calling, and you answer, whatever was being said before." | 1 of 3 answered | NAME_AFTER |

Two things about these counts. They are small, and each is one voice in one
room. And the second and third rule pull against each other: a lamp told
to stay out of other people's talk needs to be told just as plainly what
being called sounds like.

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

On 2026-10-13 the file here gave 7 and 3, 8 of 8, 7 lines ignored and both
questions answered in two runs, and 5 of 5.

## What it does not decide

How fast the lamp answers, whether it hears someone over its own voice and
how long it waits for a person who has paused are the runtime's
([duplex-listening.md](duplex-listening.md),
[conversation-latency.md](conversation-latency.md)). A character file
cannot make the lamp stop for a voice its microphone did not pick out, and
it cannot make the service answer sooner.
