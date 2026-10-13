# People talking around the lamp

What the lamp does when it is not alone with one person: several people
standing at it, or colleagues in conversation at the next desk. Status on
2026-10-13: three measures are on lamp-4ace; the third has not yet been seen
to act there. The problem is not solved.

## What happens

The lamp ends a turn after 0.6 s of quiet and its answer begins 1.0 to 1.5 s
after that. Any speech it hears in between takes the turn: the answer is
dropped (`user_interrupted`) and the new speech becomes the next thing sent
to the model. Alone with one person that is right: they paused in the middle
of a thought and went on. In a group it means the lamp never gets a word in.

On lamp-4ace on 2026-10-13 people stood at the lamp for seven minutes,
talking to it and to each other (47 turns in the service's record):

- 21 of the 47 turns were replaced by more talk, 0.01 to 1.25 s after the
  end of speech was decided, all before any of the provider's answer had
  arrived.
- One of them asked "Why you not speaking up?".
- Twice the lamp answered a scrap of their talk to each other.

Later the same day, with two colleagues in conversation beside it, six
played questions by name 25 s apart ("Hey Lamp, what is two plus two?")
went unanswered three times running in one run: every one was replaced by
their talk before the lamp spoke.

How loud a voice is does not tell these apart. People standing at the lamp
are all as loud as the person speaking to it (a median of -15 to -29 dBFS
after cancellation that morning, either way).

## What is done about it

**Voices that are too faint are not speech** (`--faint-floor-db -40`, see
[serve-runtime.md](serve-runtime.md), "How loud an utterance was"). This
removed the phantom utterances from across the office. It does nothing for
people at the lamp.

**A question put to the lamp stays its own** (the character file,
[character.md](character.md)): "you will often be asked something and then
hear other people talking to each other before you have had the chance to
answer … answer it at the first pause, once, as if the talk in between had
not happened". The model hears everything in the session, so it can do
this. With that line alone and the two colleagues still talking, six played
questions: two answered within 2 s, one 11 s late at the first pause, three
not at all (18 turns were replaced by their talk during the run). Before
the line: none of three.

**In a busy room a just-asked lamp keeps its turn**
(`crates/live/src/coordinator.rs`, `HOLD_FLOOR_US`; `Attention::busy` in
`crates/live/src/body_link.rs`). The room counts as busy when the last three
things the lamp heard went unanswered: nothing it said, in a row, with less
than half a minute of quiet since. Then, from the moment a person's speech
is judged ended until the answer begins to play, a new sound does not take
the turn: the candidate is suppressed as the lamp's own playback would be,
and the service records `floor_held` once for that turn. If no answer has
begun to arrive 1.8 s after the speech ended, the hold lapses and the next
voice takes the turn as before, so a turn the model declines to answer
costs the room at most that. Once the lamp answers, the count is zero again
and the next three turns are handled as they always were.

It is tied to a busy room because of what it costs: speech that begins
during a hold is not sent to the model at all. For one person who pauses
and goes on, that would lose the start of their second half, and the lamp
would answer the first.

## What is and is not shown

- `body_link::tests`: when a room counts as busy, and that an answer or
  half a minute of quiet ends it. The hold itself has no test of its own;
  it is a few lines in the service's main loop.
- On lamp-4ace the build with the hold answered six of six played
  questions, five within 2 s. But the colleagues had stopped talking by
  then: one turn was replaced by other talk in the whole run and
  `floor_held` was recorded zero times. That run shows the hold does no
  harm in a quiet room and nothing else.

Not solved, and known:

- Once the lamp is speaking, a voice over it is judged by the rules for
  being interrupted ([duplex-listening.md](duplex-listening.md)). People
  who carry on talking beside it can stop it in the middle of its answer.
- The lamp cannot tell who is speaking to it except by what is said. Its
  eyes report where a face is, and nothing uses that to decide whose turn
  it is.
- The lamp's light and head still react to every one of the turns it does
  not answer, until the head settles after two.
