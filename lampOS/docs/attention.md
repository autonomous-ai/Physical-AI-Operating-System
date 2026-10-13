# Whom the lamp answers

Scope: `crates/voice/src/attention.rs` and its use in the provider worker
(`crates/voice/src/provider_worker.rs`). Part of step 7 of
[brain-build.md](brain-build.md), brought forward on what the lamp did.

## The rule

The model decides what to say. Whether an answer is played is decided on
the lamp, by a rule a person can state:

| The room | The lamp plays an answer when |
|---|---|
| Busy (three things in a row it said nothing to, inside a minute; [character.md](character.md)) | its name is in what it heard, or it said something to someone in the last 45 seconds, or it began the turn itself (a hello, a look) |
| Quiet, or not yet known | always |

It is the owner's rule ("follows the room"). The model was already asked
to keep it, and mostly does.

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

## Switch

`LAMP_ATTENTION=on` in the voice unit (the worker's `--attention`). Off by
default, and off on lamp-4ace.

## Not shown

Host tests: the rule and its bookkeeping (five), and the worker with the
scripted provider in a room it is told is busy (one). The host tests give
the rule text with the name in it, which the real service does not
write: they did not catch the fault above, and the played room test
did.
